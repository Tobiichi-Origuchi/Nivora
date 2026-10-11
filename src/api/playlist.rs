use ncm_api_rs::ApiClient;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::Arc;

use super::client::{create_disguised_query, request_with_retry};
use super::session::SessionManager;
use super::song::parse_ncm_track;
use crate::common::error::AppResult;
use crate::common::model::{Playlist, Track};

/// 歌单网络 API
#[derive(Clone)]
pub struct PlaylistApi {
  session: SessionManager,
  client: Arc<RwLock<ApiClient>>,
}

impl PlaylistApi {
  pub fn new(session: SessionManager, client: Arc<RwLock<ApiClient>>) -> Self {
    Self { session, client }
  }

  /// 获取个性化推荐歌单列表
  pub async fn get_personalized_playlists(&self, limit: i64) -> AppResult<Vec<Playlist>> {
    let client = self.client.clone();
    let limit_str = limit.to_string();
    let query = create_disguised_query().param("limit", &limit_str);
    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.personalized(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let mut playlists = Vec::new();
    if let Some(list) = resp.body.get("result").and_then(Value::as_array) {
      for item in list {
        let id = item
          .get("id")
          .and_then(Value::as_i64)
          .map(|v| v.to_string())
          .unwrap_or_default();
        let title = item
          .get("name")
          .and_then(Value::as_str)
          .unwrap_or("")
          .to_string();
        let cover_url =
          crate::common::http::with_pic_size(item.get("picUrl").and_then(Value::as_str), 300);
        let track_count = item.get("trackCount").and_then(Value::as_u64).unwrap_or(0) as usize;
        let play_count = item.get("playCount").and_then(Value::as_u64).unwrap_or(0);

        playlists.push(Playlist {
          id,
          title,
          cover_url,
          description: None,
          track_count,
          creator_name: None,
          play_count,
        });
      }
    }

    Ok(playlists)
  }

  /// 获取歌单完整详情及第一批内嵌曲目
  pub async fn get_playlist_detail(&self, playlist_id: &str) -> AppResult<(Playlist, Vec<Track>)> {
    let client = self.client.clone();
    let query = create_disguised_query().param("id", playlist_id);
    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.playlist_detail(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let p_obj = resp.body.get("playlist").unwrap_or(&Value::Null);
    let id = p_obj
      .get("id")
      .and_then(Value::as_i64)
      .map(|v| v.to_string())
      .unwrap_or_else(|| playlist_id.to_string());
    let title = p_obj
      .get("name")
      .and_then(Value::as_str)
      .unwrap_or("未知歌单")
      .to_string();
    let cover_url =
      crate::common::http::with_pic_size(p_obj.get("coverImgUrl").and_then(Value::as_str), 300);
    let description = p_obj
      .get("description")
      .and_then(Value::as_str)
      .map(|s| s.to_string());
    let track_count = p_obj.get("trackCount").and_then(Value::as_u64).unwrap_or(0) as usize;
    let play_count = p_obj.get("playCount").and_then(Value::as_u64).unwrap_or(0);
    let creator_name = p_obj
      .pointer("/creator/nickname")
      .and_then(Value::as_str)
      .map(|s| s.to_string());

    let playlist = Playlist {
      id,
      title,
      cover_url,
      description,
      track_count,
      creator_name,
      play_count,
    };

    let mut tracks = Vec::new();
    if let Some(track_list) = p_obj.get("tracks").and_then(Value::as_array) {
      for item in track_list {
        if let Some(track) = parse_ncm_track(item) {
          tracks.push(track);
        }
      }
    }

    if tracks.len() < playlist.track_count
      && playlist.track_count > 0
      && let Ok(all_tracks) = self.get_playlist_tracks(&playlist.id, 0).await
      && all_tracks.len() > tracks.len()
    {
      tracks = all_tracks;
    }

    Ok((playlist, tracks))
  }

  /// 获取歌单的所有曲目
  pub async fn get_playlist_tracks(&self, playlist_id: &str, offset: i64) -> AppResult<Vec<Track>> {
    let client = self.client.clone();
    let offset_str = offset.to_string();
    let query = create_disguised_query()
      .param("id", playlist_id)
      .param("offset", &offset_str);

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.playlist_track_all(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let mut tracks = Vec::new();
    if let Some(songs) = resp.body.get("songs").and_then(Value::as_array) {
      for song in songs {
        if let Some(track) = parse_ncm_track(song) {
          tracks.push(track);
        }
      }
    }

    Ok(tracks)
  }

  /// 获取用户创建与收藏的歌单列表
  pub async fn get_user_playlists(
    &self,
    uid: i64,
    limit: i64,
    offset: i64,
  ) -> AppResult<Vec<Playlist>> {
    let client = self.client.clone();
    let uid_str = uid.to_string();
    let limit_str = limit.to_string();
    let offset_str = offset.to_string();

    let query = create_disguised_query()
      .param("uid", &uid_str)
      .param("limit", &limit_str)
      .param("offset", &offset_str);

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.user_playlist(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let mut playlists = Vec::new();
    if let Some(list) = resp.body.get("playlist").and_then(Value::as_array) {
      for item in list {
        let id = item
          .get("id")
          .and_then(Value::as_i64)
          .map(|v| v.to_string())
          .unwrap_or_default();
        let title = item
          .get("name")
          .and_then(Value::as_str)
          .unwrap_or("")
          .to_string();
        let cover_url =
          crate::common::http::with_pic_size(item.get("coverImgUrl").and_then(Value::as_str), 300);
        let track_count = item.get("trackCount").and_then(Value::as_u64).unwrap_or(0) as usize;
        let play_count = item.get("playCount").and_then(Value::as_u64).unwrap_or(0);
        let description = item
          .get("description")
          .and_then(Value::as_str)
          .map(|s| s.to_string());
        let creator_name = item
          .pointer("/creator/nickname")
          .and_then(Value::as_str)
          .map(|s| s.to_string());

        playlists.push(Playlist {
          id,
          title,
          cover_url,
          description,
          track_count,
          creator_name,
          play_count,
        });
      }
    }

    Ok(playlists)
  }

  /// 获取每日推荐歌曲
  pub async fn get_recommend_songs(&self) -> AppResult<Vec<Track>> {
    let client = self.client.clone();
    let query = create_disguised_query();
    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.recommend_songs(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let mut tracks = Vec::new();
    let song_list = resp
      .body
      .pointer("/data/dailySongs")
      .or_else(|| resp.body.get("recommend"))
      .and_then(Value::as_array);

    if let Some(list) = song_list {
      for item in list {
        if let Some(track) = parse_ncm_track(item) {
          tracks.push(track);
        }
      }
    }

    Ok(tracks)
  }

  /// 收藏或取消收藏歌单
  pub async fn subscribe_playlist(&self, playlist_id: &str, subscribe: bool) -> AppResult<bool> {
    let client = self.client.clone();
    let t_val = if subscribe { "1" } else { "2" };
    let query = create_disguised_query()
      .param("id", playlist_id)
      .param("t", t_val);

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.playlist_subscribe(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let code = resp.body.get("code").and_then(Value::as_i64).unwrap_or(0);
    Ok(code == 200)
  }

  /// 获取推荐新歌列表
  pub async fn get_recommend_newsongs(&self, limit: i64) -> AppResult<Vec<Track>> {
    let client = self.client.clone();
    let query = create_disguised_query().param("limit", &limit.to_string());
    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.personalized_newsong(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let mut tracks = Vec::new();
    if let Some(list) = resp.body.get("result").and_then(Value::as_array) {
      for item in list {
        if let Some(song_val) = item.get("song") {
          if let Some(track) = parse_ncm_track(song_val) {
            tracks.push(track);
          }
        } else if let Some(track) = parse_ncm_track(item) {
          tracks.push(track);
        }
      }
    }

    Ok(tracks)
  }
}
