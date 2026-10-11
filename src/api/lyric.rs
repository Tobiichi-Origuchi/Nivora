use ncm_api_rs::ApiClient;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::Arc;

use super::client::{create_disguised_query, request_with_retry};
use super::session::SessionManager;
use crate::common::error::AppResult;

/// 歌词网络拉取 API
#[derive(Clone)]
pub struct LyricApi {
  session: SessionManager,
  client: Arc<RwLock<ApiClient>>,
}

impl LyricApi {
  pub fn new(session: SessionManager, client: Arc<RwLock<ApiClient>>) -> Self {
    Self { session, client }
  }

  /// 获取歌词原始文本 (LRC逐行, YRC逐字, 翻译)
  pub async fn get_lyrics(
    &self,
    song_id: &str,
  ) -> AppResult<(Option<String>, Option<String>, Option<String>)> {
    let client = self.client.clone();
    let query = create_disguised_query().param("id", song_id);
    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.lyric_new(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let lrc = resp
      .body
      .pointer("/lrc/lyric")
      .and_then(Value::as_str)
      .map(|s| s.to_string());
    let yrc = resp
      .body
      .pointer("/yrc/lyric")
      .and_then(Value::as_str)
      .map(|s| s.to_string());
    let tlyric = resp
      .body
      .pointer("/tlyric/lyric")
      .and_then(Value::as_str)
      .map(|s| s.to_string());

    Ok((lrc, yrc, tlyric))
  }
}
