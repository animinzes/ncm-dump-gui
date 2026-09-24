//! 输出路径解析：命名模板渲染、非法字符清洗、音乐库归档、去重、源文件处理。

use std::path::{Path, PathBuf};

use crate::config::{DedupePolicy, OutputMode, SourceAction};

/// 命名模板可用占位符的上下文
#[derive(Debug, Clone, Default)]
pub struct NameContext {
    pub artist: String,
    pub title: String,
    pub album: String,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<u32>,
    pub bitrate: Option<i64>,
}

impl NameContext {
    /// Windows 非法字符 `\/:*?"<>|` 与控制字符替换为 _，去首尾空白与结尾点号
    fn value_of(&self, key: &str) -> Option<String> {
        let raw = match key {
            "artist" => self.artist.as_str(),
            "title" => self.title.as_str(),
            "album" => self.album.as_str(),
            "track" => return self.track.map(|t| format!("{t:02}")),
            "disc" => return self.disc.map(|d| format!("{d:02}")),
            "year" => return self.year.map(|y| y.to_string()),
            "bitrate" => {
                return self.bitrate.map(|b| format!("{}", (b / 1000).max(1)))
            }
            _ => return None,
        };
        Some(sanitize_component(raw))
    }
}

/// 清洗单个文件名/目录名成分（模板值都会经过这里）
pub fn sanitize_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '|' | '>'
        ) || c.is_control()
        {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    // Windows 不允许结尾点号/空白
    let trimmed = out.trim().trim_end_matches('.').to_string();
    if trimmed.is_empty() {
        "未知".to_string()
    } else {
        trimmed
    }
}

/// 渲染命名模板（如 "{artist} - {title}"）；未知占位符原样保留
pub fn render_template(template: &str, ctx: &NameContext) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut key = String::new();
            let mut closed = false;
            for k in chars.by_ref() {
                if k == '}' {
                    closed = true;
                    break;
                }
                key.push(k);
            }
            if closed {
                match ctx.value_of(&key) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('{');
                        out.push_str(&key);
                        out.push('}');
                    }
                }
            } else {
                out.push('{');
                out.push_str(&key);
            }
        } else {
            out.push(c);
        }
    }
    if out.trim().is_empty() {
        "未知".to_string()
    } else {
        out
    }
}

/// 目标路径冲突的解析结果
pub enum TargetAction {
    /// 直接写入此路径
    Write(PathBuf),
    /// 目标已存在，按策略跳过
    Skip(PathBuf),
}

/// 根据输出模式计算目标目录
/// - Source: 源文件所在目录
/// - Custom: 自定义目录
/// - Library: 库根目录 + archive_structure 渲染（如 "{artist}/{album}"）
pub fn resolve_target_dir(
    mode: OutputMode,
    custom_dir: &str,
    library_dir: &str,
    archive_structure: &str,
    source_path: &Path,
    ctx: &NameContext,
) -> PathBuf {
    match mode {
        OutputMode::Source => source_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
        OutputMode::Custom => PathBuf::from(if custom_dir.is_empty() {
            "."
        } else {
            custom_dir
        }),
        OutputMode::Library => {
            let root = if library_dir.is_empty() {
                "."
            } else {
                library_dir
            };
            let sub = render_template(archive_structure, ctx);
            // 归档结构里每一段目录名都要清洗（渲染时值已清洗，这里处理手动写死的部分）
            let sub = sub
                .split(['/', '\\'])
                .map(sanitize_component)
                .filter(|s| !s.is_empty() && s != ".")
                .collect::<Vec<_>>()
                .join(std::path::MAIN_SEPARATOR_STR);
            PathBuf::from(root).join(sub)
        }
    }
}

/// 在目标目录里为 stem 计算最终文件名（含去重策略）。
/// 返回 (完整路径, 动作)；extension 由调用方传（如 "mp3"）。
pub fn resolve_target_file(
    dir: &Path,
    stem: &str,
    ext: &str,
    dedupe: DedupePolicy,
) -> (PathBuf, TargetAction) {
    let stem = sanitize_component(stem);
    let primary = dir.join(format!("{stem}.{ext}"));
    if !primary.exists() {
        return (primary.clone(), TargetAction::Write(primary));
    }
    match dedupe {
        DedupePolicy::Skip => (primary.clone(), TargetAction::Skip(primary)),
        DedupePolicy::Overwrite => (primary.clone(), TargetAction::Write(primary)),
        DedupePolicy::Rename => {
            let mut n = 2u32;
            loop {
                let candidate = dir.join(format!("{stem} ({n}).{ext}"));
                if !candidate.exists() {
                    return (candidate.clone(), TargetAction::Write(candidate));
                }
                n += 1;
                if n > 9999 {
                    // 兜底：极端情况下直接覆盖主名
                    return (primary, TargetAction::Write(dir.join(format!("{stem}.{ext}"))));
                }
            }
        }
    }
}

/// 处理转换成功后的源文件
pub fn handle_source(path: &Path, action: SourceAction) -> Result<(), String> {
    match action {
        SourceAction::Keep => Ok(()),
        SourceAction::Recycle => trash::delete(path).map_err(|e| format!("移入回收站失败: {e}")),
        SourceAction::Delete => std::fs::remove_file(path).map_err(|e| format!("删除源文件失败: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> NameContext {
        NameContext {
            artist: "周杰 伦".into(),
            title: "晴天".into(),
            album: "叶惠美".into(),
            track: Some(3),
            disc: Some(1),
            year: Some(2003),
            bitrate: Some(999000),
        }
    }

    #[test]
    fn sanitize_illegal_chars() {
        assert_eq!(sanitize_component(r#"a/b\c:d*e?f"g<h>i|j"#), "a_b_c_d_e_f_g_h_i_j");
        assert_eq!(sanitize_component("结尾点..."), "结尾点");
        assert_eq!(sanitize_component("  空白  "), "空白");
        assert_eq!(sanitize_component("///"), "___"); // 非法字符逐个替换为 _
        assert_eq!(sanitize_component(""), "未知");
    }

    #[test]
    fn render_basic_templates() {
        let c = ctx();
        assert_eq!(render_template("{title}", &c), "晴天");
        assert_eq!(render_template("{artist} - {title}", &c), "周杰 伦 - 晴天");
        assert_eq!(render_template("{album}/{track} {title}", &c), "叶惠美/03 晴天");
        assert_eq!(render_template("{year} - {bitrate}kbps", &c), "2003 - 999kbps");
        assert_eq!(render_template("{unknown} {title}", &c), "{unknown} 晴天");
        assert_eq!(render_template("   ", &c), "未知");
    }

    #[test]
    fn render_with_missing_fields() {
        let c = NameContext {
            title: "无名曲".into(),
            ..Default::default()
        };
        assert_eq!(render_template("{artist} - {title}", &c), "未知 - 无名曲");
        assert_eq!(render_template("{track}. {title}", &c), "{track}. 无名曲");
    }

    #[test]
    fn target_file_dedupe() {
        let dir = std::env::temp_dir().join(format!("ncm-lib-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 不存在 → Write
        let (p, TargetAction::Write(w)) =
            resolve_target_file(&dir, "song", "mp3", DedupePolicy::Skip)
        else {
            panic!("expect write");
        };
        assert_eq!(w, dir.join("song.mp3"));
        std::fs::write(&p, b"x").unwrap();

        // 已存在 → Skip
        let (_, action) = resolve_target_file(&dir, "song", "mp3", DedupePolicy::Skip);
        assert!(matches!(action, TargetAction::Skip(_)));

        // 已存在 + Overwrite → Write 同一路径
        let (p2, action) = resolve_target_file(&dir, "song", "mp3", DedupePolicy::Overwrite);
        assert!(matches!(action, TargetAction::Write(_)));
        assert_eq!(p2, p);

        // 已存在 + Rename → song (2).mp3
        let (p3, action) = resolve_target_file(&dir, "song", "mp3", DedupePolicy::Rename);
        match action {
            TargetAction::Write(w) => assert_eq!(w, dir.join("song (2).mp3")),
            _ => panic!("expect rename write"),
        }
        assert_eq!(p3, dir.join("song (2).mp3"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn library_target_dir() {
        let c = ctx();
        let src = Path::new(r"C:\downloads\test.ncm");
        let dir = resolve_target_dir(
            OutputMode::Library,
            "",
            r"D:\music",
            "{artist}/{album}",
            src,
            &c,
        );
        let expected = PathBuf::from(r"D:\music").join(format!("周杰 伦{}叶惠美", std::path::MAIN_SEPARATOR));
        assert_eq!(dir, expected);

        // 源目录模式
        let dir = resolve_target_dir(OutputMode::Source, "", "", "{artist}", src, &c);
        assert_eq!(dir, PathBuf::from(r"C:\downloads"));

        // 自定义目录模式
        let dir = resolve_target_dir(OutputMode::Custom, r"E:\out", "", "{artist}", src, &c);
        assert_eq!(dir, PathBuf::from(r"E:\out"));
    }
}
