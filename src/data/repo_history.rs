use std::time::{SystemTime, UNIX_EPOCH};

use super::db::Database;
use crate::common::error::AppResult;
use crate::common::model::{FavoriteItem, PlayHistoryItem, Track, UserListeningStats};

/// 播放历史与收藏足迹存储仓库
pub struct HistoryRepo;

impl HistoryRepo {
  /// 记录单曲播放足迹
  pub fn record_play(db: &Database, track: &Track, listened_ms: u64) -> AppResult<()> {
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);

    let source = format!("{:?}", track.source);
    let track_json = serde_json::to_string(track)?;

    let conn = db.conn();
    conn.execute(
      r#"
            INSERT INTO play_history (track_id, source, started_at, listened_ms, track_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
      rusqlite::params![track.id, source, now, listened_ms as i64, track_json],
    )?;

    Ok(())
  }

  /// 获取最近播放历史列表
  pub fn list_recent_plays(db: &Database, limit: usize) -> AppResult<Vec<PlayHistoryItem>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
      r#"
            SELECT id, track_id, source, started_at, listened_ms, track_json
            FROM play_history
            ORDER BY started_at DESC
            LIMIT ?1
            "#,
    )?;

    let rows = stmt.query_map([limit as i64], |row| {
      let id: i64 = row.get(0)?;
      let track_id: String = row.get(1)?;
      let source: String = row.get(2)?;
      let started_at: i64 = row.get(3)?;
      let listened_ms: i64 = row.get(4)?;
      let track_json: String = row.get(5)?;

      let track: Track = serde_json::from_str(&track_json).unwrap_or_default();

      Ok(PlayHistoryItem {
        id,
        track_id,
        source,
        started_at,
        listened_ms: listened_ms.max(0) as u64,
        track,
      })
    })?;

    let mut items = Vec::new();
    for r in rows {
      items.push(r?);
    }
    Ok(items)
  }

  /// 记录收藏/取消收藏动作
  pub fn record_favorite(db: &Database, track: &Track, action: &str) -> AppResult<()> {
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);

    let source = format!("{:?}", track.source);
    let track_json = serde_json::to_string(track)?;

    let conn = db.conn();
    conn.execute(
      r#"
            INSERT INTO favorite_history (track_id, source, action, at, track_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
      rusqlite::params![track.id, source, action, now, track_json],
    )?;

    Ok(())
  }

  /// 判断指定歌曲当前是否被收藏
  pub fn is_favorite(db: &Database, track_id: &str, source: &str) -> AppResult<bool> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
      r#"
            SELECT action FROM favorite_history
            WHERE track_id = ?1 AND source = ?2
            ORDER BY at DESC, id DESC
            LIMIT 1
            "#,
    )?;

    let action: Option<String> = stmt
      .query_row(rusqlite::params![track_id, source], |row| row.get(0))
      .ok();

    Ok(action.as_deref() == Some("like"))
  }

  /// 查询当前所有已收藏的曲目列表
  pub fn list_favorites(db: &Database, limit: usize) -> AppResult<Vec<FavoriteItem>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
      r#"
            SELECT f.id, f.track_id, f.source, f.action, f.at, f.track_json
            FROM favorite_history f
            INNER JOIN (
                SELECT track_id, source, MAX(id) as max_id
                FROM favorite_history
                GROUP BY track_id, source
            ) latest ON f.id = latest.max_id
            WHERE f.action = 'like'
            ORDER BY f.at DESC
            LIMIT ?1
            "#,
    )?;

    let rows = stmt.query_map([limit as i64], |row| {
      let id: i64 = row.get(0)?;
      let track_id: String = row.get(1)?;
      let source: String = row.get(2)?;
      let action: String = row.get(3)?;
      let at: i64 = row.get(4)?;
      let track_json: String = row.get(5)?;

      let track: Track = serde_json::from_str(&track_json).unwrap_or_default();

      Ok(FavoriteItem {
        id,
        track_id,
        source,
        action,
        at,
        track,
      })
    })?;

    let mut items = Vec::new();
    for r in rows {
      items.push(r?);
    }
    Ok(items)
  }

  /// 获取用户综合听歌统计数据
  pub fn get_listening_stats(db: &Database) -> AppResult<UserListeningStats> {
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);
    let week_ago = now - 7 * 86400;
    let two_weeks_ago = now - 14 * 86400;

    let conn = db.conn();

    // 1. 本周听歌毫秒数
    let weekly_ms: i64 = conn
      .query_row(
        "SELECT COALESCE(SUM(listened_ms), 0) FROM play_history WHERE started_at >= ?1",
        [week_ago],
        |row| row.get(0),
      )
      .unwrap_or(0);

    // 2. 上周听歌毫秒数
    let prev_week_ms: i64 = conn.query_row(
            "SELECT COALESCE(SUM(listened_ms), 0) FROM play_history WHERE started_at >= ?1 AND started_at < ?2",
            [two_weeks_ago, week_ago],
            |row| row.get(0),
        ).unwrap_or(0);

    // 3. 累计收听歌曲总数
    let total_plays: i64 = conn
      .query_row("SELECT COUNT(*) FROM play_history", [], |row| row.get(0))
      .unwrap_or(0);

    // 4. 当前已收藏歌曲总数
    let total_favs: i64 = conn
      .query_row(
        r#"
            SELECT COUNT(*) FROM (
                SELECT track_id, source, MAX(id) as max_id
                FROM favorite_history
                GROUP BY track_id, source
            ) latest
            INNER JOIN favorite_history f ON f.id = latest.max_id
            WHERE f.action = 'like'
            "#,
        [],
        |row| row.get(0),
      )
      .unwrap_or(0);

    // 5. 连续收听天数
    let mut stmt = conn.prepare(
            r#"
            SELECT DISTINCT strftime('%Y-%m-%d', datetime(started_at, 'unixepoch', 'localtime')) as play_date
            FROM play_history
            ORDER BY play_date DESC
            LIMIT 60
            "#,
        )?;
    let play_dates: Vec<String> = stmt
      .query_map([], |row| row.get(0))?
      .filter_map(|r| r.ok())
      .collect();

    let streak_days = calculate_streak(&play_dates);

    let weekly_hours = weekly_ms as f64 / (1000.0 * 3600.0);
    let weekly_hours_str = format!("{:.1}", weekly_hours);

    let weekly_growth_str = if prev_week_ms > 0 {
      let growth = (weekly_ms - prev_week_ms) as f64 / prev_week_ms as f64 * 100.0;
      if growth >= 0.0 {
        format!("+{:.1}%", growth)
      } else {
        format!("{:.1}%", growth)
      }
    } else if weekly_ms > 0 {
      "+100%".to_string()
    } else {
      "0.0%".to_string()
    };

    let favorite_ratio_str = if total_plays > 0 {
      format!(
        "{:.1}%",
        (total_favs as f64 / total_plays as f64 * 100.0).min(100.0)
      )
    } else {
      "0.0%".to_string()
    };

    Ok(UserListeningStats {
      weekly_hours_str,
      total_tracks_played: total_plays.max(0) as u64,
      streak_days,
      total_favorites: total_favs.max(0) as u64,
      weekly_growth_str,
      favorite_ratio_str,
    })
  }
}

fn calculate_streak(dates: &[String]) -> u32 {
  if dates.is_empty() {
    return 0;
  }
  let today = chrono::Local::now().date_naive();
  let mut streak = 0;
  let mut expected_date = today;

  if let Some(first_str) = dates.first() {
    if let Ok(first_date) = chrono::NaiveDate::parse_from_str(first_str, "%Y-%m-%d") {
      if first_date == today {
        expected_date = today;
      } else if first_date == today.pred_opt().unwrap_or(today) {
        expected_date = first_date;
      } else {
        return 0;
      }
    } else {
      return 0;
    }
  }

  for date_str in dates {
    if let Ok(d) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
      if d == expected_date {
        streak += 1;
        expected_date = expected_date.pred_opt().unwrap_or(expected_date);
      } else {
        break;
      }
    }
  }
  streak
}
