use std::time::{SystemTime, UNIX_EPOCH};

use super::db::Database;
use crate::common::error::AppResult;
use crate::common::model::{Album, Artist, Playlist, Track};
use crate::common::types::TrackSource;

/// 本地歌单数据仓库
pub struct PlaylistRepo;

impl PlaylistRepo {
  /// 创建本地歌单
  pub fn create_playlist(
    db: &Database,
    id: &str,
    title: &str,
    description: Option<&str>,
    cover: Option<&str>,
  ) -> AppResult<()> {
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map(|d| d.as_secs() as i64)
      .unwrap_or(0);

    let conn = db.conn();
    conn.execute(
      r#"
            INSERT INTO playlists (id, type, title, description, cover, created_at, updated_at)
            VALUES (?1, 'local', ?2, ?3, ?4, ?5, ?6)
            "#,
      rusqlite::params![id, title, description, cover, now, now],
    )?;

    Ok(())
  }

  /// 获取所有本地歌单
  pub fn list_playlists(db: &Database) -> AppResult<Vec<Playlist>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
      r#"
            SELECT p.id, p.title, p.description, p.cover,
                   COUNT(pt.track_id) as track_count
            FROM playlists p
            LEFT JOIN playlist_tracks pt ON p.id = pt.playlist_id
            GROUP BY p.id
            ORDER BY p.updated_at DESC
            "#,
    )?;

    let rows = stmt.query_map([], |row| {
      let id: String = row.get(0)?;
      let title: String = row.get(1)?;
      let description: Option<String> = row.get(2)?;
      let cover: Option<String> = row.get(3)?;
      let track_count: i64 = row.get(4)?;

      Ok(Playlist {
        id,
        title,
        cover_url: cover,
        description,
        track_count: track_count.max(0) as usize,
        creator_name: Some("本地用户".to_string()),
        play_count: 0,
      })
    })?;

    let mut playlists = Vec::new();
    for r in rows {
      playlists.push(r?);
    }
    Ok(playlists)
  }

  /// 获取歌单关联的曲目信息
  pub fn get_playlist_tracks(db: &Database, playlist_id: &str) -> AppResult<Vec<Track>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
      r#"
            SELECT t.id, t.path, t.title, t.artists, t.album, t.duration, t.cover
            FROM playlist_tracks pt
            INNER JOIN tracks t ON pt.track_id = t.id
            WHERE pt.playlist_id = ?1
            ORDER BY pt.position ASC
            "#,
    )?;

    let rows = stmt.query_map([playlist_id], |row| {
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
