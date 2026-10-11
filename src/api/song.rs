use ncm_api_rs::ApiClient;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::Arc;

use super::client::{create_disguised_query, request_with_retry};
use super::session::SessionManager;
use crate::common::error::AppResult;
use crate::common::model::{Album, Artist, Track};
use crate::common::types::{AudioQualityLevel, TrackSource};

/// 单曲网络 API
#[derive(Clone)]
pub struct SongApi {
  session: SessionManager,
  client: Arc<RwLock<ApiClient>>,
}

impl SongApi {
  pub fn new(session: SessionManager, client: Arc<RwLock<ApiClient>>) -> Self {
    Self { session, client }
  }

  /// 获取单曲播放直链
  pub async fn get_song_url(
    &self,
    song_id: &str,
    quality: AudioQualityLevel,
  ) -> AppResult<Option<String>> {
    let client = self.client.clone();
    let level = quality.as_level_str();
    let query = create_disguised_query()
      .param("id", song_id)
      .param("level", level);

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.song_url_v1(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let url = resp
      .body
      .pointer("/data/0/url")
      .and_then(Value::as_str)
      .filter(|s| !s.is_empty())
      .map(|s| s.to_string());

    Ok(url)
  }

  /// 拉取单曲详情元数据
  pub async fn get_song_detail(&self, ids: &[&str]) -> AppResult<Vec<Track>> {
    if ids.is_empty() {
      return Ok(Vec::new());
    }

    let client = self.client.clone();
    let ids_joined = ids.join(",");
    let query = create_disguised_query().param("ids", &ids_joined);

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.song_detail(&query).await }
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

  /// 标记喜爱/取消喜爱歌曲
  pub async fn like_song(&self, song_id: &str, like: bool) -> AppResult<bool> {
    let client = self.client.clone();
    let like_str = if like { "true" } else { "false" };
    let query = create_disguised_query()
      .param("id", song_id)
      .param("like", like_str);

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.song_like(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let code = resp.body.get("code").and_then(Value::as_i64).unwrap_or(0);
    Ok(code == 200)
  }

  /// 获取相似歌曲推荐
  pub async fn get_similar_songs(&self, song_id: &str) -> AppResult<Vec<Track>> {
    let client = self.client.clone();
    let query = create_disguised_query().param("id", song_id);

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.simi_song(&query).await }
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

  /// 获取私人 FM 推荐曲目候选流
  pub async fn get_personal_fm(&self) -> AppResult<Vec<Track>> {
    let client = self.client.clone();
    let query = create_disguised_query();

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.personal_fm(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let mut tracks = Vec::new();
    if let Some(items) = resp.body.get("data").and_then(Value::as_array) {
      for item in items {
        if let Some(track) = parse_ncm_track(item) {
          tracks.push(track);
        }
      }
    }

    Ok(tracks)
  }

  /// 听歌打卡上报
  pub async fn scrobble(&self, song_id: &str, source_id: &str, time_secs: i64) -> AppResult<()> {
    let client = self.client.read().clone();
    let time_str = time_secs.to_string();
    let query = create_disguised_query()
      .param("id", song_id)
      .param("sourceid", source_id)
      .param("time", &time_str);

    let resp = client.scrobble(&query).await?;
    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    Ok(())
  }
}

/// 解析网易云歌曲 JSON 节点为 Track 实体
pub fn parse_ncm_track(item: &Value) -> Option<Track> {
  let id = item.get("id")?.as_i64()?.to_string();
  let title = item
    .get("name")
    .and_then(Value::as_str)
    .unwrap_or("")
    .to_string();
  let duration_ms = item
    .get("dt")
    .or_else(|| item.get("duration"))
    .and_then(Value::as_u64)
    .unwrap_or(0);
  let fee = item.get("fee").and_then(Value::as_i64).unwrap_or(0) as i32;

  // 提取歌手列表
  let ar_array = item
    .get("ar")
    .or_else(|| item.get("artists"))
    .and_then(Value::as_array);
  let artists = ar_array
    .map(|list| {
      list
        .iter()
        .map(|ar| {
          let ar_id = ar
            .get("id")
            .and_then(Value::as_i64)
            .map(|v| v.to_string())
            .unwrap_or_default();
          let ar_name = ar
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
          Artist {
            id: ar_id,
            name: ar_name,
            avatar: None,
          }
        })
        .collect::<Vec<_>>()
    })
    .unwrap_or_default();

  // 提取专辑信息
  let al_obj = item.get("al").or_else(|| item.get("album"));
  let raw_cover = al_obj.and_then(|al| al.get("picUrl").and_then(Value::as_str));
  let cover_url = crate::common::http::with_pic_size(raw_cover, 300);
  let cover_original = crate::common::http::with_pic_size(raw_cover, 1024);

  let album = al_obj.map(|al| {
    let al_id = al
      .get("id")
      .and_then(Value::as_i64)
      .map(|v| v.to_string())
      .unwrap_or_default();
    let al_name = al
      .get("name")
      .and_then(Value::as_str)
      .unwrap_or("")
      .to_string();
    Album {
      id: al_id,
      name: al_name,
      cover: cover_url.clone(),
    }
  });

  Some(Track {
    id,
    title,
    artists,
    album,
    duration_ms,
    cover_url,
    cover_original,
    source: TrackSource::Ncm,
    local_path: None,
    fee,
  })
}
