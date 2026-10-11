use ncm_api_rs::ApiClient;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::Arc;

use super::client::{create_disguised_query, request_with_retry};
use super::session::SessionManager;
use crate::common::error::AppResult;
use crate::common::model::{Album, Artist, Track};
use crate::common::types::TrackSource;

/// 歌手信息与热门作品 API
#[derive(Clone)]
pub struct ArtistApi {
  session: SessionManager,
  client: Arc<RwLock<ApiClient>>,
}

impl ArtistApi {
  pub fn new(session: SessionManager, client: Arc<RwLock<ApiClient>>) -> Self {
    Self { session, client }
  }

  /// 获取歌手基本资料与热门单曲
  pub async fn get_artist_detail(
    &self,
    artist_id: &str,
  ) -> AppResult<(Artist, String, Vec<Track>)> {
    let client = self.client.clone();
    let query = create_disguised_query().param("id", artist_id);

    // 1. 获取歌手头部详细信息
    let resp_detail = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.artist_detail(&query).await }
    })
    .await?;
    if !resp_detail.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp_detail.cookie);
    }

    let artist_obj = resp_detail.body.pointer("/data/artist");
    let name = artist_obj
      .and_then(|a| a.get("name"))
      .and_then(Value::as_str)
      .unwrap_or("未知歌手")
      .to_string();
    let avatar = artist_obj
      .and_then(|a| a.get("cover").or_else(|| a.get("avatar")))
      .and_then(Value::as_str)
      .map(|s| s.to_string());
    let desc = artist_obj
      .and_then(|a| a.get("briefDesc"))
      .and_then(Value::as_str)
      .unwrap_or("")
      .to_string();

    let artist = Artist {
      id: artist_id.to_string(),
      name,
      avatar,
    };

    // 2. 获取歌手热门歌曲 (Top 50)
    let resp_songs = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.artist_top_song(&query).await }
    })
    .await?;
    if !resp_songs.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp_songs.cookie);
    }

    let mut tracks = Vec::new();
    if let Some(songs) = resp_songs.body.get("songs").and_then(Value::as_array) {
      for song in songs {
        let id = song
          .get("id")
          .and_then(Value::as_i64)
          .unwrap_or(0)
          .to_string();
        let title = song
          .get("name")
          .and_then(Value::as_str)
          .unwrap_or("未知曲目")
          .to_string();
        let duration_ms = song.get("dt").and_then(Value::as_i64).unwrap_or(0) as u64;
        let fee = song.get("fee").and_then(Value::as_i64).unwrap_or(0) as i32;

        let raw_cover = song
          .get("al")
          .and_then(|al| al.get("picUrl").and_then(Value::as_str));
        let cover_url = crate::common::http::with_pic_size(raw_cover, 300);
        let cover_original = crate::common::http::with_pic_size(raw_cover, 1024);

        let album = song.get("al").map(|al| {
          let a_id = al
            .get("id")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .to_string();
          let a_name = al
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
          Album {
            id: a_id,
            name: a_name,
            cover: cover_url.clone(),
          }
        });

        let mut song_artists = Vec::new();
        if let Some(ar_arr) = song.get("ar").and_then(Value::as_array) {
          for ar in ar_arr {
            let ar_id = ar
              .get("id")
              .and_then(Value::as_i64)
              .unwrap_or(0)
              .to_string();
            let ar_name = ar
              .get("name")
              .and_then(Value::as_str)
              .unwrap_or("")
              .to_string();
            song_artists.push(Artist {
              id: ar_id,
              name: ar_name,
              avatar: None,
            });
          }
        }

        tracks.push(Track {
          id,
          title,
          artists: song_artists,
          album,
          duration_ms,
          cover_url,
          cover_original,
          source: TrackSource::Ncm,
          local_path: None,
          fee,
        });
      }
    }

    Ok((artist, desc, tracks))
  }
}
