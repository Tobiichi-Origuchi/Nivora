use std::time::{SystemTime, UNIX_EPOCH};

use super::db::Database;
use crate::common::error::AppResult;
use crate::common::model::{DownloadTask, Track};

/// 离线下载任务数据仓库
pub struct DownloadRepo;

impl DownloadRepo {
  /// 创建新的下载任务
  pub fn create_task(db: &Database, task: &DownloadTask) -> AppResult<()> {
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);

    let track_json = serde_json::to_string(&task.track)?;
    let conn = db.conn();
    conn.execute(
      r#"
            INSERT INTO download_tasks (
                task_id, track_json, quality_level, status, received, total,
                file_path, error_code, tag_warning, created_at, finished_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(task_id) DO UPDATE SET
                track_json = excluded.track_json,
                quality_level = excluded.quality_level,
                status = excluded.status,
                received = excluded.received,
                total = excluded.total,
                file_path = excluded.file_path,
                error_code = excluded.error_code,
                tag_warning = excluded.tag_warning,
                created_at = excluded.created_at,
                finished_at = excluded.finished_at
            "#,
      rusqlite::params![
        task.task_id,
        track_json,
        task.quality_level,
        task.status,
        task.received as i64,
        task.total as i64,
        task.file_path,
        task.error_code,
        if task.tag_warning { 1 } else { 0 },
        now,
        task.finished_at
      ],
    )?;

    Ok(())
  }

  /// 更新任务下载进度
  pub fn update_progress(db: &Database, task_id: &str, received: u64, total: u64) -> AppResult<()> {
    let conn = db.conn();
    conn.execute(
      r#"
            UPDATE download_tasks
            SET received = ?1, total = ?2, status = 'downloading'
            WHERE task_id = ?3
            "#,
      rusqlite::params![received as i64, total as i64, task_id],
    )?;
    Ok(())
  }

  /// 标记任务下载并标签写入完成
  pub fn complete_task(db: &Database, task_id: &str, file_path: &str) -> AppResult<()> {
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);

    let conn = db.conn();
    conn.execute(
      r#"
            UPDATE download_tasks
            SET status = 'completed', file_path = ?1, finished_at = ?2
            WHERE task_id = ?3
            "#,
      rusqlite::params![file_path, now, task_id],
    )?;
    Ok(())
  }

  /// 标记任务失败
  pub fn fail_task(db: &Database, task_id: &str, error_code: &str) -> AppResult<()> {
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);

    let conn = db.conn();
    conn.execute(
      r#"
            UPDATE download_tasks
            SET status = 'failed', error_code = ?1, finished_at = ?2
            WHERE task_id = ?3
            "#,
      rusqlite::params![error_code, now, task_id],
    )?;
    Ok(())
  }

  /// 列出所有下载任务
  pub fn list_tasks(db: &Database) -> AppResult<Vec<DownloadTask>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
      r#"
            SELECT task_id, track_json, quality_level, status, received, total,
                   file_path, error_code, tag_warning, created_at, finished_at
            FROM download_tasks
            ORDER BY created_at DESC
            "#,
    )?;

    let rows = stmt.query_map([], |row| {
      let task_id: String = row.get(0)?;
      let track_json: String = row.get(1)?;
      let quality_level: String = row.get(2)?;
      let status: String = row.get(3)?;
      let received: i64 = row.get(4)?;
      let total: i64 = row.get(5)?;
      let file_path: Option<String> = row.get(6)?;
      let error_code: Option<String> = row.get(7)?;
      let tag_warning: i32 = row.get(8)?;
      let created_at: i64 = row.get(9)?;
      let finished_at: Option<i64> = row.get(10)?;

      let track: Track = serde_json::from_str(&track_json).unwrap_or_default();

      Ok(DownloadTask {
        task_id,
        track,
        quality_level,
        status,
        received: received.max(0) as u64,
        total: total.max(0) as u64,
        file_path,
        error_code,
        tag_warning: tag_warning == 1,
        created_at,
        finished_at,
      })
    })?;

    let mut tasks = Vec::new();
    for r in rows {
      tasks.push(r?);
    }
    Ok(tasks)
  }
}
