use ncm_api_rs::ApiClient;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::Arc;

use super::client::{create_disguised_query, request_with_retry};
use super::session::SessionManager;
use super::song::parse_ncm_track;
use crate::common::error::AppResult;
use crate::common::model::{Playlist, Track};

/// 搜索网络 API
#[derive(Clone)]
pub struct SearchApi {
  session: SessionManager,
  client: Arc<RwLock<ApiClient>>,
}

impl SearchApi {
  pub fn new(session: SessionManager, client: Arc<RwLock<ApiClient>>) -> Self {
    Self { session, client }
  }

  /// 搜索单曲列表
  pub async fn search_songs(
    &self,
    keyword: &str,
    limit: i64,
    offset: i64,
  ) -> AppResult<Vec<Track>> {
    let client = self.client.clone();
    let limit_str = limit.to_string();
    let offset_str = offset.to_string();
    let query = create_disguised_query()
      .param("keywords", keyword)
      .param("type", "1") // 1: 单曲
      .param("limit", &limit_str)
      .param("offset", &offset_str);

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.cloudsearch(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let mut tracks = Vec::new();
    if let Some(song_list) = resp.body.pointer("/result/songs").and_then(Value::as_array) {
      for item in song_list {
        if let Some(track) = parse_ncm_track(item) {
          tracks.push(track);
        }
      }
    }

    Ok(tracks)
  }

  /// 搜索歌单列表
  pub async fn search_playlists(
    &self,
    keyword: &str,
    limit: i64,
    offset: i64,
  ) -> AppResult<Vec<Playlist>> {
    let client = self.client.clone();
    let limit_str = limit.to_string();
    let offset_str = offset.to_string();
    let query = create_disguised_query()
      .param("keywords", keyword)
      .param("type", "1000") // 1000: 歌单
      .param("limit", &limit_str)
      .param("offset", &offset_str);

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.cloudsearch(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let mut playlists = Vec::new();
    if let Some(list) = resp
      .body
      .pointer("/result/playlists")
      .and_then(Value::as_array)
    {
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

  /// 获取搜索联想补全建议
  pub async fn search_suggest(&self, keyword: &str) -> AppResult<Vec<String>> {
    let client = self.client.clone();
    let query = create_disguised_query()
      .param("keywords", keyword)
      .param("type", "mobile");

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.search_suggest(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let mut suggestions = Vec::new();
    if let Some(list) = resp
      .body
      .pointer("/result/allMatch")
      .and_then(Value::as_array)
    {
      for item in list {
        if let Some(kw) = item.get("keyword").and_then(Value::as_str) {
          suggestions.push(kw.to_string());
        }
      }
    }

    Ok(suggestions)
  }

  /// 获取热门搜索词列表
  pub async fn search_hot_detail(&self) -> AppResult<Vec<String>> {
    let client = self.client.clone();
    let query = create_disguised_query();
    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.search_hot_detail(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let mut words = Vec::new();
    if let Some(list) = resp.body.get("data").and_then(Value::as_array) {
      for item in list {
        if let Some(word) = item.get("searchWord").and_then(Value::as_str) {
          words.push(word.to_string());
        }
      }
    }

    Ok(words)
  }

  /// 获取默认搜索占位词
  pub async fn search_default(&self) -> AppResult<String> {
    let client = self.client.clone();
    let query = create_disguised_query();
    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.search_default(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let word = resp
      .body
      .pointer("/data/showKeyword")
      .or_else(|| resp.body.pointer("/data/realkeyword"))
      .and_then(Value::as_str)
      .unwrap_or("搜索音乐、歌手、歌单")
      .to_string();

    Ok(word)
  }
}
