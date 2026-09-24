//! Tauri 命令层：配置读写、路径扫描、网易云目录探测、并行转换（逐文件进度事件）。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rayon::prelude::*;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::config::{self, AppConfig, CoverStrategy};
use crate::library::{self, NameContext, TargetAction};
use crate::metadata::tags::{self, TagData};
use crate::ncm::crypt::{is_ncm_path, NcmFile};
use crate::netease::{year_from_ms, Lyrics, NeteaseClient};

/// 全局状态
pub struct AppState {
    pub config: Mutex<AppConfig>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Mutex::new(config),
        }
    }
}

/// 逐文件进度事件（前端监听 "convert-progress"）
#[derive(Clone, Serialize)]
pub struct ProgressEvent {
    pub file: String,
    /// converting | success | failed | skipped
    pub status: String,
    pub message: Option<String>,
    pub output: Option<String>,
}

#[derive(Serialize)]
pub struct ConvertSummary {
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
}

fn emit(app: &AppHandle, event: ProgressEvent) {
    let _ = app.emit("convert-progress", event);
}

// ---------- 配置 ----------

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_config(state: State<'_, AppState>, config: AppConfig) -> Result<(), String> {
    let mut cfg = config;
    cfg.convert.parallel_workers = cfg.convert.parallel_workers.min(64);
    let path = config::config_path().ok_or("无法定位配置目录")?;
    config::save_atomic(&path, &cfg).map_err(|e| format!("保存配置失败: {e}"))?;
    *state.config.lock().unwrap() = cfg;
    Ok(())
}

// ---------- 路径扫描 ----------

/// 展开传入路径：.ncm 文件直接收录；目录按 recursive 扫描
#[tauri::command]
pub fn add_paths(paths: Vec<String>, recursive: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in paths {
        let path = PathBuf::from(&p);
        if path.is_dir() {
            collect_ncm(&path, recursive, &mut out);
        } else if path.is_file() && is_ncm_path(&path) {
            out.push(p);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn collect_ncm(dir: &Path, recursive: bool, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                collect_ncm(&path, recursive, out);
            }
        } else if is_ncm_path(&path) {
            out.push(path.to_string_lossy().into_owned());
        }
    }
}

/// 记住目录（last_used：上次使用；netease：快速导入历史，去重、上限 10）
#[tauri::command]
pub fn remember_dir(state: State<'_, AppState>, path: String, kind: String) -> Result<(), String> {
    if path.is_empty() {
        return Ok(());
    }
    let mut cfg = state.config.lock().unwrap().clone();
    match kind.as_str() {
        "last_used" => cfg.scan.last_used_dir = path,
        "netease" => {
            cfg.scan.netease_dirs.retain(|d| d != &path);
            cfg.scan.netease_dirs.insert(0, path);
            cfg.scan.netease_dirs.truncate(10);
        }
        _ => return Err(format!("未知目录类别: {kind}")),
    }
    let save_cfg = cfg.clone();
    if let Some(p) = config::config_path() {
        config::save_atomic(&p, &save_cfg).ok();
    }
    *state.config.lock().unwrap() = cfg;
    Ok(())
}

// ---------- 网易云客户端目录探测 ----------

/// 探测本机可能的网易云下载/缓存目录（注册表 + 常见位置，仅返回真实存在的）
#[tauri::command]
pub fn detect_netease_dirs() -> Vec<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 注册表探测客户端安装路径（HKCU / HKLM）
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
        use winreg::RegKey;
        for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            for sub in [
                r"Software\NetEase Cloud Music",
                r"Software\Netease\Cloud Music",
                r"SOFTWARE\WOW6432Node\NetEase Cloud Music",
            ] {
                if let Ok(key) = RegKey::predef(root).open_subkey_with_flags(sub, KEY_READ) {
                    for value in ["InstallDir", "installDir", "Path"] {
                        if let Ok(dir) = key.get_value::<String, _>(value) {
                            candidates.push(PathBuf::from(&dir));
                        }
                    }
                }
            }
        }
    }

    // 常见下载/缓存位置
    if let Some(music) = dirs::audio_dir() {
        candidates.push(music);
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Downloads"));
        candidates.push(home.join("Music").join("CloudMusic"));
    }
    if let Some(local) = dirs::data_local_dir() {
        candidates.push(local.join(r"Netease\CloudMusic\Cache"));
    }
    candidates.push(PathBuf::from(r"D:\CloudMusic"));

    // 去重 + 只保留存在的目录
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|p| p.is_dir() && seen.insert(p.clone()))
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

// ---------- 转换 ----------

/// 批量转换：rayon 并行，逐文件发进度事件，返回汇总
#[tauri::command]
pub fn convert(app: AppHandle, state: State<'_, AppState>, files: Vec<String>) -> ConvertSummary {
    let cfg = state.config.lock().unwrap().clone();

    let workers = match cfg.convert.parallel_workers {
        0 => std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
        n => (n as usize).min(64),
    };

    let need_net = cfg.metadata.net_enabled
        || cfg.metadata.lyrics_enabled
        || cfg.metadata.cover_strategy == CoverStrategy::Auto;
    let client = if need_net {
        Some(NeteaseClient::new(cfg.network.timeout_secs, cfg.network.retries))
    } else {
        None
    };

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers.max(1))
        .build()
        .unwrap_or_else(|_| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(4)
                .build()
                .expect("fallback rayon pool")
        });

    let counts = pool.install(|| {
        files
            .par_iter()
            .map(|file| convert_one(&app, client.as_ref(), &cfg, file))
            .fold(
                || (0usize, 0usize, 0usize),
                |(s, f, k), status| match status {
                    Status::Success => (s + 1, f, k),
                    Status::Failed => (s, f + 1, k),
                    Status::Skipped => (s, f, k + 1),
                },
            )
            .reduce(
                || (0, 0, 0),
                |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2),
            )
    });

    ConvertSummary {
        success: counts.0,
        failed: counts.1,
        skipped: counts.2,
    }
}

enum Status {
    Success,
    Failed,
    Skipped,
}

fn convert_one(app: &AppHandle, client: Option<&NeteaseClient>, cfg: &AppConfig, file: &str) -> Status {
    emit(
        app,
        ProgressEvent {
            file: file.to_string(),
            status: "converting".into(),
            message: None,
            output: None,
        },
    );
    match convert_inner(client, cfg, file) {
        Ok(outcome) => {
            let (status, output) = match &outcome {
                Outcome::Success(p) => ("success", Some(p.to_string_lossy().into_owned())),
                Outcome::Skipped(p) => ("skipped", Some(p.to_string_lossy().into_owned())),
            };
            emit(
                app,
                ProgressEvent {
                    file: file.to_string(),
                    status: status.into(),
                    message: match &outcome {
                        Outcome::Skipped(_) => Some("目标文件已存在，按策略跳过".into()),
                        _ => None,
                    },
                    output,
                },
            );
            match outcome {
                Outcome::Success(_) => Status::Success,
                Outcome::Skipped(_) => Status::Skipped,
            }
        }
        Err(msg) => {
            emit(
                app,
                ProgressEvent {
                    file: file.to_string(),
                    status: "failed".into(),
                    message: Some(msg),
                    output: None,
                },
            );
            Status::Failed
        }
    }
}

#[derive(Debug)]
enum Outcome {
    Success(PathBuf),
    Skipped(PathBuf),
}

fn convert_inner(
    client: Option<&NeteaseClient>,
    cfg: &AppConfig,
    file: &str,
) -> Result<Outcome, String> {
    let path = Path::new(file);
    let mut ncm = NcmFile::open(path).map_err(|e| e.0)?;
    let meta = ncm.metadata.clone().unwrap_or_default();

    // ---- 网络补全（音轨号/碟号/年份/专辑歌手 + 歌词 + 封面兜底）----
    let mut track: Option<u32> = None;
    let mut disc: Option<u32> = None;
    let mut year: Option<u32> = None;
    let mut album_artist: Option<String> = None;
    let mut api_cover_url: Option<String> = None;
    let mut lyrics: Option<String> = None;

    if let (Some(client), Some(music_id)) = (client, meta.music_id) {
        if cfg.metadata.net_enabled {
            if let Some(detail) = client.song_detail(music_id) {
                track = detail.track_no;
                disc = detail.disc;
                year = detail.publish_time_ms.and_then(year_from_ms);
                album_artist = detail.album_artist.clone();
                api_cover_url = detail.album_pic_url;
            }
        }
        if cfg.metadata.lyrics_enabled {
            if let Some(l) = client.lyric(music_id) {
                lyrics = Some(combine_lyrics(l));
            }
        }
    }

    let mut cover = ncm.cover.clone();
    if cover.is_none() && cfg.metadata.cover_strategy == CoverStrategy::Auto {
        if let Some(client) = client {
            // 优先元数据内嵌的封面直链，再退到 API 的专辑封面
            let url = meta.album_pic_url.clone().or(api_cover_url);
            if let Some(url) = url {
                cover = client.download_image(&url);
            }
        }
    }

    // ---- 命名与目标路径 ----
    let file_stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = if meta.music_name.is_empty() {
        file_stem
    } else {
        meta.music_name.clone()
    };
    let artist = if meta.artists.is_empty() {
        "未知歌手".to_string()
    } else {
        meta.artist_joined()
    };
    let album = if meta.album.is_empty() {
        "未知专辑".to_string()
    } else {
        meta.album.clone()
    };

    let ctx = NameContext {
        artist: artist.clone(),
        title: title.clone(),
        album: album.clone(),
        track,
        disc,
        year,
        bitrate: meta.bitrate,
    };

    let target_dir = library::resolve_target_dir(
        cfg.output.mode,
        &cfg.output.custom_dir,
        &cfg.output.library_dir,
        &cfg.output.archive_structure,
        path,
        &ctx,
    );
    std::fs::create_dir_all(&target_dir).map_err(|e| format!("创建输出目录失败: {e}"))?;

    let stem = library::render_template(&cfg.output.naming_template, &ctx);
    let (target, action) = library::resolve_target_file(
        &target_dir,
        &stem,
        ncm.format().extension(),
        cfg.output.dedupe,
    );
    let target_path = match action {
        TargetAction::Skip(_) => return Ok(Outcome::Skipped(target)),
        TargetAction::Write(p) => p,
    };

    // ---- 解密写出（先写临时文件再改名，避免半成品占住正式名）----
    let tmp = target_dir.join(format!(".{stem}.part"));
    ncm.dump_to(&tmp).map_err(|e| e.0)?;
    if target_path.exists() {
        // overwrite 策略或 rename 兜底
        std::fs::remove_file(&target_path).map_err(|e| format!("覆盖目标失败: {e}"))?;
    }
    std::fs::rename(&tmp, &target_path).map_err(|e| format!("落盘输出文件失败: {e}"))?;

    // ---- 标签 ----
    let effective_album_artist = if cfg.metadata.write_album_artist {
        // API 的专辑歌手优先；无则单歌手时用歌手本人
        album_artist.or_else(|| {
            if meta.artists.len() == 1 {
                Some(meta.artists[0].clone())
            } else {
                None
            }
        })
    } else {
        None
    };
    let tag_data = TagData {
        title,
        artist,
        album,
        album_artist: effective_album_artist,
        track,
        disc,
        year,
        lyrics,
        cover,
    };
    tags::apply_tags(&target_path, &tag_data).map_err(|e| e.0)?;

    // ---- 源文件处理 ----
    library::handle_source(path, cfg.convert.source_after_success)?;

    Ok(Outcome::Success(target_path))
}

/// 原文 + 翻译合并（原文后空一行接翻译）
fn combine_lyrics(l: Lyrics) -> String {
    match l.translation {
        Some(t) if !t.trim().is_empty() => format!("{}\n\n{}", l.original.trim_end(), t),
        _ => l.original,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OutputMode;

    #[test]
    fn lyrics_combination() {
        let l = Lyrics {
            original: "[00:01.00] hello\n".into(),
            translation: Some("[00:01.00] 你好\n".into()),
        };
        let combined = combine_lyrics(l);
        assert!(combined.contains("hello"));
        assert!(combined.contains("你好"));
    }

    #[test]
    fn end_to_end_convert_with_tags() {
        // 用参考仓库的 test.ncm 走完整管线（无网络场景：net 全关）
        let ncm_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ncmdump/test/test.ncm");
        if !ncm_path.exists() {
            eprintln!("skip: test.ncm not found");
            return;
        }
        let out_dir = std::env::temp_dir().join(format!("ncm-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out_dir);
        std::fs::create_dir_all(&out_dir).unwrap();

        let mut cfg = AppConfig::default();
        cfg.output.mode = OutputMode::Custom;
        cfg.output.custom_dir = out_dir.to_string_lossy().into_owned();
        cfg.metadata.net_enabled = false;
        cfg.metadata.lyrics_enabled = false;
        cfg.metadata.cover_strategy = CoverStrategy::Embedded;
        let file = ncm_path.to_string_lossy().into_owned();

        match convert_inner(None, &cfg, &file) {
            Ok(Outcome::Success(p)) => {
                assert!(p.exists());
                let ext = p.extension().unwrap().to_string_lossy().into_owned();
                assert!(ext == "mp3" || ext == "flac", "unexpected ext {ext}");
                // 回读标签
                use lofty::file::TaggedFileExt;
                let tag = lofty::probe::Probe::open(&p)
                    .unwrap()
                    .read()
                    .unwrap()
                    .primary_tag()
                    .cloned();
                if let Some(tag) = tag {
                    use lofty::prelude::Accessor;
                    assert!(!tag.title().unwrap_or_default().is_empty(), "标题应已写入");
                    assert!(!tag.artist().unwrap_or_default().is_empty(), "歌手应已写入");
                    assert!(!tag.album().unwrap_or_default().is_empty(), "专辑应已写入");
                    assert!(
                        !tag.pictures().is_empty(),
                        "test.ncm 内嵌封面应已写入标签"
                    );
                } else {
                    panic!("标签读取失败");
                }
            }
            other => panic!("转换失败: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&out_dir);
    }
}
