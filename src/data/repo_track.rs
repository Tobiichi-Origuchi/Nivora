use std::time::{SystemTime, UNIX_EPOCH};

use super::db::Database;
use crate::common::error::AppResult;
use crate::common::model::{Album, Artist, Track};
use crate::common::types::TrackSource;

/// 本地音乐曲目数据仓库
pub struct TrackRepo;

impl TrackRepo {
  /// 插入或更新本地曲目元数据
  pub fn upsert_track(
    db: &Database,
    track: &Track,
    file_size: i64,
    codec: Option<&str>,
    sample_rate: Option<u32>,
    bit_rate: Option<u32>,
  ) -> AppResult<()> {
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);

    let artists_json = serde_json::to_string(&track.artists)?;
    let album_name = track.album.as_ref().map(|a| a.name.clone());
    let path = track.local_path.clone().unwrap_or_else(|| track.id.clone());

    let conn = db.conn();
    conn.execute(
      r#"
            INSERT INTO tracks (
                id, path, title, artists, album, duration, cover,
                codec, sample_rate, bit_rate, file_size, scanned_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                path = excluded.path,
                title = excluded.title,
                artists = excluded.artists,
                album = excluded.album,
                duration = excluded.duration,
                cover = excluded.cover,
                codec = excluded.codec,
                sample_rate = excluded.sample_rate,
                bit_rate = excluded.bit_rate,
                file_size = excluded.file_size,
                scanned_at = excluded.scanned_at
            "#,
      rusqlite::params![
        track.id,
        path,
        track.title,
        artists_json,
        album_name,
        track.duration_ms as i64,
        track.cover_url,
        codec,
        sample_rate,
        bit_rate,
        file_size,
        now
      ],
    )?;

    Ok(())
  }

  /// 查询所有本地曲目
  pub fn list_tracks(db: &Database) -> AppResult<Vec<Track>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
      r#"
            SELECT id, path, title, artists, album, duration, cover
            FROM tracks
            ORDER BY scanned_at DESC
            "#,
    )?;

    let rows = stmt.query_map([], |row| {
      let id: String = row.get(0)?;
      let path: String = row.get(1)?;
      let title: String = row.get(2)?;
      let artists_json: String = row.get(3)?;
      let album_name: Option<String> = row.get(4)?;
      let duration: i64 = row.get(5)?;
      let cover: Option<String> = row.get(6)?;

      let artists: Vec<Artist> = serde_json::from_str(&artists_json).unwrap_or_default();
      let album = album_name.map(|name| Album {
        id: String::new(),
        name,
        cover: cover.clone(),
      });

      Ok(Track {
        id,
        title,
        artists,
        album,
        duration_ms: duration.max(0) as u64,
        cover_url: cover,
        cover_original: None,
        source: TrackSource::Local,
        local_path: Some(path),
        fee: 0,
      })
    })?;

    let mut tracks = Vec::new();
    for r in rows {
      tracks.push(r?);
    }
    Ok(tracks)
  }
}
