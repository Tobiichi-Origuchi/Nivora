use directories_next::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::error::{AppError, AppResult};
use super::types::{AudioQualityLevel, PlayMode};

pub const QUALIFIER: &str = "com";
pub const ORGANIZATION: &str = "splayer";
pub const APPLICATION: &str = "Nivora";
const CONFIG_FILE_NAME: &str = "config.json";

fn default_theme_color() -> String {
  "#fe7971".to_string()
}

fn default_custom_color_hex() -> String {
  "#fe7971".to_string()
}

fn default_eq_preset() -> String {
  "平直 (Flat)".to_string()
}

/// 应用持久化配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
  /// 默认首选音频品质
  pub audio_quality: AudioQualityLevel,
  /// 主音量 (0.0 ~ 1.0)
  pub volume: f32,
  /// 音量平滑渐变时长（毫秒，0 表示禁用）
  pub fade_duration_ms: u64,
  /// 界面暗色模式开关 (true: 深色, false: 浅色)
  pub dark_mode: bool,
  /// 自定义下载保存目录
  pub download_dir: Option<PathBuf>,
  /// 桌面歌词启用开关
  pub desktop_lyric_enabled: bool,
  /// 本地音乐扫描目录列表
  pub music_folders: Vec<PathBuf>,
  /// 主题强调色 HEX (如 "#fe7971")
  #[serde(default = "default_theme_color")]
  pub theme_color: String,
  /// 自定义主题色十六进制
  #[serde(default = "default_custom_color_hex")]
  pub custom_color_hex: String,
  /// 封面自适应动态取色开关
  #[serde(default)]
  pub dynamic_color_enabled: bool,
  /// 循环播放模式
  #[serde(default)]
  pub play_mode: PlayMode,
  /// 侧边栏折叠状态
  #[serde(default)]
  pub sidebar_collapsed: bool,
  /// 均衡器预设风格
  #[serde(default = "default_eq_preset")]
  pub eq_preset: String,
  /// 均衡器 10 频段增益
  #[serde(default)]
  pub eq_gains: Vec<f32>,
}

impl Default for AppConfig {
  fn default() -> Self {
    Self {
      audio_quality: AudioQualityLevel::ExHigh,
      volume: 0.8,
      fade_duration_ms: 300,
      dark_mode: true,
      download_dir: None,
      desktop_lyric_enabled: false,
      music_folders: Vec::new(),
      theme_color: default_theme_color(),
      custom_color_hex: default_custom_color_hex(),
      dynamic_color_enabled: false,
      play_mode: PlayMode::Sequence,
      sidebar_collapsed: false,
      eq_preset: default_eq_preset(),
      eq_gains: vec![0.0; 10],
    }
  }
}

impl AppConfig {
  /// 获取应用标准目录结构
  fn project_dirs() -> AppResult<ProjectDirs> {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
      .ok_or_else(|| AppError::Config("无法获取操作系统标准用户目录".to_string()))
  }

  /// 配置存储目录
  pub fn config_dir() -> AppResult<PathBuf> {
    let dirs = Self::project_dirs()?;
    let path = dirs.config_dir().to_path_buf();
    if !path.exists() {
      fs::create_dir_all(&path)?;
    }
    Ok(path)
  }

  /// 本地数据与数据库存储目录
  pub fn data_dir() -> AppResult<PathBuf> {
    let dirs = Self::project_dirs()?;
    let path = dirs.data_dir().to_path_buf();
    if !path.exists() {
      fs::create_dir_all(&path)?;
    }
    Ok(path)
  }

  /// 封面与音频流缓存目录
  pub fn cache_dir() -> AppResult<PathBuf> {
    let dirs = Self::project_dirs()?;
    let path = dirs.cache_dir().to_path_buf();
    if !path.exists() {
      fs::create_dir_all(&path)?;
    }
    Ok(path)
  }

  /// 从磁盘加载配置，不存在则创建默认配置并保存
  pub fn load() -> Self {
    match Self::try_load() {
      Ok(config) => config,
      Err(err) => {
        tracing::warn!("未能成功加载配置文件，已恢复默认配置: {err}");
        let default_cfg = Self::default();
        let _ = default_cfg.save();
        default_cfg
      }
    }
  }

  fn try_load() -> AppResult<Self> {
    let config_path = Self::config_dir()?.join(CONFIG_FILE_NAME);
    if !config_path.exists() {
      let default_cfg = Self::default();
      default_cfg.save()?;
      return Ok(default_cfg);
    }
    let content = fs::read_to_string(&config_path)?;
    let config = serde_json::from_str(&content)?;
    Ok(config)
  }

  /// 将配置保存至磁盘
  pub fn save(&self) -> AppResult<()> {
    let config_path = Self::config_dir()?.join(CONFIG_FILE_NAME);
    let content = serde_json::to_string_pretty(self)?;
    fs::write(config_path, content)?;
    Ok(())
  }

  /// 将配置导出至 JSON 文件（用于手动备份，不参与运行时自动同步）
  pub fn export_to_file(&self, path: &std::path::Path) -> AppResult<()> {
    let content = serde_json::to_string_pretty(self)?;
    fs::write(path, content)?;
    Ok(())
  }
}
