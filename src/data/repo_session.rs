use rusqlite::OptionalExtension;
use std::time::{SystemTime, UNIX_EPOCH};

use super::db::Database;
use crate::common::error::AppResult;

/// 平台账号会话持久化存储仓库
pub struct SessionRepo;

impl SessionRepo {
  /// 获取保存的 Cookies 字符串
  pub fn get_cookies(db: &Database, platform: &str) -> AppResult<Option<String>> {
    let conn = db.conn();
    let mut stmt = conn.prepare("SELECT cookies FROM account_sessions WHERE platform = ?1")?;
    let cookies: Option<String> = stmt.query_row([platform], |row| row.get(0)).optional()?;
    Ok(cookies)
  }

  /// 保存或更新指定平台的 Cookies 凭据
  pub fn save_cookies(db: &Database, platform: &str, cookies: &str) -> AppResult<()> {
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);

    let conn = db.conn();
    conn.execute(
      r#"
            INSERT INTO account_sessions (platform, cookies, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(platform) DO UPDATE SET
                cookies = excluded.cookies,
                updated_at = excluded.updated_at
            "#,
      rusqlite::params![platform, cookies, now],
    )?;

    Ok(())
  }

  /// 清除指定平台的会话凭据
  pub fn clear_session(db: &Database, platform: &str) -> AppResult<()> {
    let conn = db.conn();
    conn.execute(
      "DELETE FROM account_sessions WHERE platform = ?1",
      [platform],
    )?;
    Ok(())
  }
}
