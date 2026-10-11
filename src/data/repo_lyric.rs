use rusqlite::OptionalExtension;
use std::time::{SystemTime, UNIX_EPOCH};

use super::db::Database;
use crate::common::error::AppResult;

/// 离线歌词缓存数据仓库
pub struct LyricRepo;

impl LyricRepo {
  /// 从本地缓存查询歌词数据
  pub fn get_lyric(db: &Database, platform: &str, platform_id: &str) -> AppResult<Option<String>> {
    let conn = db.conn();
    let mut stmt =
      conn.prepare("SELECT data FROM lyric_cache WHERE platform = ?1 AND platform_id = ?2")?;

    let data: Option<String> = stmt
      .query_row([platform, platform_id], |row| row.get(0))
      .optional()?;

    Ok(data)
  }

  /// 将歌词数据保存至离线缓存
  pub fn save_lyric(db: &Database, platform: &str, platform_id: &str, data: &str) -> AppResult<()> {
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);

    let conn = db.conn();
    conn.execute(
      r#"
            INSERT INTO lyric_cache (platform, platform_id, data, fetched_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(platform, platform_id) DO UPDATE SET
                data = excluded.data,
                fetched_at = excluded.fetched_at
            "#,
      rusqlite::params![platform, platform_id, data, now],
    )?;

    Ok(())
  }
}
