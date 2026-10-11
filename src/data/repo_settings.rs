use rusqlite::OptionalExtension;
use std::time::{SystemTime, UNIX_EPOCH};

use super::db::Database;
use crate::common::error::AppResult;

/// 全局应用设置与偏好仓库
pub struct SettingsRepo;

impl SettingsRepo {
  /// 获取键对应的值
  pub fn get(db: &Database, key: &str) -> AppResult<Option<String>> {
    let conn = db.conn();
    let mut stmt = conn.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
    let val = stmt.query_row([key], |row| row.get(0)).optional()?;
    Ok(val)
  }

  /// 设置键值并持久化落盘
  pub fn set(db: &Database, key: &str, value: &str) -> AppResult<()> {
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);
    let conn = db.conn();
    conn.execute(
      r#"
            INSERT INTO app_settings (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at
            "#,
      rusqlite::params![key, value, now],
    )?;
    Ok(())
  }

  /// 读取布尔值，失败返回默认值
  pub fn get_bool(db: &Database, key: &str, default: bool) -> bool {
    match Self::get(db, key) {
      Ok(Some(v)) => v.parse::<bool>().unwrap_or(default),
      _ => default,
    }
  }

  /// 读取字符串值，失败返回默认值
  pub fn get_string(db: &Database, key: &str, default: &str) -> String {
    match Self::get(db, key) {
      Ok(Some(v)) => v,
      _ => default.to_string(),
    }
  }

  /// 读取浮点数，失败返回默认值
  pub fn get_f32(db: &Database, key: &str, default: f32) -> f32 {
    match Self::get(db, key) {
      Ok(Some(v)) => v.parse::<f32>().unwrap_or(default),
      _ => default,
    }
  }

  /// 读取整数，失败返回默认值
  pub fn get_i32(db: &Database, key: &str, default: i32) -> i32 {
    match Self::get(db, key) {
      Ok(Some(v)) => v.parse::<i32>().unwrap_or(default),
      _ => default,
    }
  }

  /// 读取 JSON 反序列化对象
  pub fn get_json<T: serde::de::DeserializeOwned>(db: &Database, key: &str) -> Option<T> {
    let val_str = Self::get(db, key).ok()??;
    serde_json::from_str(&val_str).ok()
  }

  /// 将对象序列化为 JSON 并存入数据库
  pub fn set_json<T: serde::Serialize>(db: &Database, key: &str, val: &T) -> AppResult<()> {
    let json_str = serde_json::to_string(val)?;
    Self::set(db, key, &json_str)
  }

  /// 从本地唯一权威持久化 SQLite 数据库加载应用配置
  pub fn load_config(db: &Database) -> crate::common::config::AppConfig {
    use crate::common::config::AppConfig;
    use crate::common::types::{AudioQualityLevel, PlayMode};
    use std::path::PathBuf;

    let has_settings = Self::get(db, "theme_color").ok().flatten().is_some()
      || Self::get(db, "dark_mode").ok().flatten().is_some();

    if has_settings {
      let mut cfg = AppConfig::default();

      cfg.theme_color = Self::get_string(db, "theme_color", &cfg.theme_color);
      cfg.custom_color_hex = Self::get_string(db, "custom_color_hex", &cfg.custom_color_hex);
      cfg.dynamic_color_enabled =
        Self::get_bool(db, "dynamic_color_enabled", cfg.dynamic_color_enabled);
      cfg.dark_mode = Self::get_bool(db, "dark_mode", cfg.dark_mode);
      cfg.volume = Self::get_f32(db, "volume", cfg.volume);
      cfg.sidebar_collapsed = Self::get_bool(db, "sidebar_collapsed", cfg.sidebar_collapsed);
      cfg.desktop_lyric_enabled =
        Self::get_bool(db, "desktop_lyric_enabled", cfg.desktop_lyric_enabled);
      cfg.eq_preset = Self::get_string(db, "eq_preset", &cfg.eq_preset);

      if let Some(q) = Self::get_json::<AudioQualityLevel>(db, "audio_quality") {
        cfg.audio_quality = q;
      }
      if let Some(pm) = Self::get_json::<PlayMode>(db, "play_mode") {
        cfg.play_mode = pm;
      }
      if let Some(gains) = Self::get_json::<Vec<f32>>(db, "eq_gains") {
        cfg.eq_gains = gains;
      }
      if let Some(folders) = Self::get_json::<Vec<PathBuf>>(db, "music_folders") {
        cfg.music_folders = folders;
      }
      if let Some(down_dir) = Self::get_json::<Option<PathBuf>>(db, "download_dir") {
        cfg.download_dir = down_dir;
      }

      cfg
    } else {
      // 首次启动时从初始默认配置建立 SQLite 权威基线
      let cfg = AppConfig::default();
      let _ = Self::save_config(db, &cfg);
      cfg
    }
  }

  /// 同步持久化配置写入单一权威库 SQLite 数据库
  pub fn save_config(db: &Database, config: &crate::common::config::AppConfig) -> AppResult<()> {
    Self::set(db, "theme_color", &config.theme_color)?;
    Self::set(db, "custom_color_hex", &config.custom_color_hex)?;
    Self::set(
      db,
      "dynamic_color_enabled",
      &config.dynamic_color_enabled.to_string(),
    )?;
    Self::set(db, "dark_mode", &config.dark_mode.to_string())?;
    Self::set(db, "volume", &config.volume.to_string())?;
    Self::set(
      db,
      "sidebar_collapsed",
      &config.sidebar_collapsed.to_string(),
    )?;
    Self::set(
      db,
      "desktop_lyric_enabled",
      &config.desktop_lyric_enabled.to_string(),
    )?;
    Self::set(db, "eq_preset", &config.eq_preset)?;
    Self::set_json(db, "audio_quality", &config.audio_quality)?;
    Self::set_json(db, "play_mode", &config.play_mode)?;
    Self::set_json(db, "eq_gains", &config.eq_gains)?;
    Self::set_json(db, "music_folders", &config.music_folders)?;
    Self::set_json(db, "download_dir", &config.download_dir)?;

    Ok(())
  }
}
