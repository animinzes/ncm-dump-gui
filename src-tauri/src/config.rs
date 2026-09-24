//! 应用配置：schema、默认值、原子读写。
//!
//! 保存形式：单个 JSON 文件，位于系统应用配置目录
//! （Windows: %APPDATA%\com.animinzes.ncm-dump-gui\config.json）。
//! 人类可读、可手改；缺字段自动回退默认值（serde default），旧配置永不崩。

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const APP_IDENTIFIER: &str = "com.animinzes.ncm-dump-gui";
const CONFIG_FILE: &str = "config.json";

/// 用户个人音乐库根目录（初始设置，已按用户选择定制）
const DEFAULT_LIBRARY_DIR: &str = r"D:\File\Properties\Anthology of life\私の音楽\音乐";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub version: u32,
    pub output: OutputConfig,
    pub convert: ConvertConfig,
    pub metadata: MetadataConfig,
    pub network: NetworkConfig,
    pub scan: ScanConfig,
    pub ui: UiConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            output: OutputConfig::default(),
            convert: ConvertConfig::default(),
            metadata: MetadataConfig::default(),
            network: NetworkConfig::default(),
            scan: ScanConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct OutputConfig {
    pub mode: OutputMode,
    pub custom_dir: String,
    pub library_dir: String,
    pub naming_template: String,
    pub archive_structure: String,
    pub dedupe: DedupePolicy,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            mode: OutputMode::Library,
            custom_dir: String::new(),
            library_dir: DEFAULT_LIBRARY_DIR.to_string(),
            naming_template: "{title}".to_string(),
            archive_structure: "{artist}/{album}".to_string(),
            dedupe: DedupePolicy::Skip,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    Source,
    Custom,
    Library,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DedupePolicy {
    Skip,
    Overwrite,
    Rename,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ConvertConfig {
    pub parallel_workers: u32,
    pub source_after_success: SourceAction,
}

impl Default for ConvertConfig {
    fn default() -> Self {
        Self {
            parallel_workers: 4,
            source_after_success: SourceAction::Keep,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceAction {
    Keep,
    Recycle,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MetadataConfig {
    pub net_enabled: bool,
    pub lyrics_enabled: bool,
    pub cover_strategy: CoverStrategy,
    pub write_album_artist: bool,
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self {
            net_enabled: true,
            lyrics_enabled: true,
            cover_strategy: CoverStrategy::Auto,
            write_album_artist: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverStrategy {
    Auto,
    Embedded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NetworkConfig {
    pub timeout_secs: u64,
    pub retries: u32,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 10,
            retries: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ScanConfig {
    pub recursive: bool,
    pub netease_dirs: Vec<String>,
    pub last_used_dir: String,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            recursive: true,
            netease_dirs: Vec::new(),
            last_used_dir: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiConfig {
    pub language: String,
    pub window_width: u32,
    pub window_height: u32,
    pub remember_file_list: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            language: "zh-CN".to_string(),
            window_width: 960,
            window_height: 640,
            remember_file_list: false,
        }
    }
}

/// 配置文件路径（系统应用配置目录下）
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(APP_IDENTIFIER).join(CONFIG_FILE))
}

/// 从指定路径加载配置；文件不存在或解析失败时返回默认值。
/// 传入 None 时使用默认路径。
pub fn load(path: Option<&Path>) -> AppConfig {
    let path = match path.map(|p| p.to_path_buf()).or_else(config_path) {
        Some(p) => p,
        None => return AppConfig::default(),
    };
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

/// 原子写入配置：先写临时文件再 rename，防止写一半损坏。
pub fn save_atomic(path: &Path, config: &AppConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(json.as_bytes())?;
        f.sync_all().ok();
    }
    // Windows 上 rename 到已存在文件会失败，先移除旧文件
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&tmp, path)
}

/// 加载并在文件不存在时写入初始配置（应用启动时调用）
pub fn load_or_init(path: Option<&Path>) -> AppConfig {
    let path = match path.map(|p| p.to_path_buf()).or_else(config_path) {
        Some(p) => p,
        None => return AppConfig::default(),
    };
    if !path.exists() {
        let config = AppConfig::default();
        save_atomic(&path, &config).ok();
        return config;
    }
    load(Some(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_spec() {
        let c = AppConfig::default();
        assert_eq!(c.version, 1);
        assert_eq!(c.output.mode, OutputMode::Library);
        assert_eq!(c.output.library_dir, DEFAULT_LIBRARY_DIR);
        assert_eq!(c.output.naming_template, "{title}");
        assert_eq!(c.output.archive_structure, "{artist}/{album}");
        assert_eq!(c.output.dedupe, DedupePolicy::Skip);
        assert_eq!(c.convert.parallel_workers, 4);
        assert_eq!(c.convert.source_after_success, SourceAction::Keep);
        assert!(c.metadata.net_enabled);
        assert!(c.metadata.lyrics_enabled);
        assert_eq!(c.metadata.cover_strategy, CoverStrategy::Auto);
        assert!(c.metadata.write_album_artist);
        assert_eq!(c.network.timeout_secs, 10);
        assert_eq!(c.network.retries, 2);
        assert!(c.scan.recursive);
        assert_eq!(c.ui.language, "zh-CN");
        assert_eq!(c.ui.window_width, 960);
        assert_eq!(c.ui.window_height, 640);
        assert!(!c.ui.remember_file_list);
    }

    #[test]
    fn first_launch_creates_full_config() {
        let dir = std::env::temp_dir().join(format!("ncm-cfg-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join(CONFIG_FILE);
        let cfg = load_or_init(Some(&path));
        assert!(path.exists());
        let text = fs::read_to_string(&path).unwrap();
        // 关键字段全部落盘
        assert!(text.contains("\"library_dir\""));
        assert!(text.contains("\"naming_template\""));
        assert!(text.contains("\"source_after_success\""));
        assert!(text.contains("\"remember_file_list\""));
        assert_eq!(cfg, AppConfig::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("ncm-cfg-test2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CONFIG_FILE);
        // 只写一个字段，其余应回退默认值
        fs::write(&path, "{\"output\":{\"naming_template\":\"{title} - x\"}}").unwrap();
        let cfg = load(Some(&path));
        assert_eq!(cfg.output.naming_template, "{title} - x");
        assert_eq!(cfg.output.mode, OutputMode::Library);
        assert!(cfg.metadata.net_enabled);
        assert_eq!(cfg.network.timeout_secs, 10);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_falls_back() {
        let dir = std::env::temp_dir().join(format!("ncm-cfg-test3-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CONFIG_FILE);
        fs::write(&path, "not json at all {").unwrap();
        let cfg = load(Some(&path));
        assert_eq!(cfg, AppConfig::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn enum_serialization_is_snake_case() {
        let c = AppConfig::default();
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"mode\":\"library\""));
        assert!(json.contains("\"source_after_success\":\"keep\""));
        assert!(json.contains("\"dedupe\":\"skip\""));
        assert!(json.contains("\"cover_strategy\":\"auto\""));
    }
}
