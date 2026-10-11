use ncm_api_rs::ApiClient;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::Arc;

use super::client::{create_disguised_query, request_with_retry};
use super::session::SessionManager;
use crate::common::error::AppResult;
use crate::common::model::Comment;

/// 歌曲与资源评论 API
#[derive(Clone)]
pub struct CommentApi {
  session: SessionManager,
  client: Arc<RwLock<ApiClient>>,
}

impl CommentApi {
  pub fn new(session: SessionManager, client: Arc<RwLock<ApiClient>>) -> Self {
    Self { session, client }
  }

  /// 获取单曲的热门与最新评论
  pub async fn get_music_comments(
    &self,
    song_id: &str,
    limit: i64,
    offset: i64,
  ) -> AppResult<Vec<Comment>> {
    let client = self.client.clone();
    let query = create_disguised_query()
      .param("id", song_id)
      .param("limit", &limit.to_string())
      .param("offset", &offset.to_string());

    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.comment_music(&query).await }
    })
    .await?;

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let mut result = Vec::new();

    // 优先使用 hotComments，若为空则使用常规 comments
    let comments_array = resp
      .body
      .get("hotComments")
      .and_then(Value::as_array)
      .filter(|arr| !arr.is_empty())
      .or_else(|| resp.body.get("comments").and_then(Value::as_array));

    if let Some(arr) = comments_array {
      for item in arr {
        let id = item.get("commentId").and_then(Value::as_i64).unwrap_or(0);
        let content = item
          .get("content")
          .and_then(Value::as_str)
          .unwrap_or("")
          .to_string();
        let user_nickname = item
          .pointer("/user/nickname")
          .and_then(Value::as_str)
          .unwrap_or("云音乐用户")
          .to_string();
        let user_avatar = item
          .pointer("/user/avatarUrl")
          .and_then(Value::as_str)
          .unwrap_or("")
          .to_string();
        let time_str = item
          .get("timeStr")
          .and_then(Value::as_str)
          .map(|s| s.to_string())
          .unwrap_or_else(|| {
            if let Some(time_ms) = item.get("time").and_then(Value::as_i64) {
              let secs = time_ms / 1000;
              if let Some(dt) = chrono::DateTime::from_timestamp(secs, 0) {
                return dt.format("%Y-%m-%d %H:%M").to_string();
              }
            }
            "未知时间".to_string()
          });
        let liked_count = item.get("likedCount").and_then(Value::as_i64).unwrap_or(0) as i32;

        result.push(Comment {
          id,
          user_nickname,
          user_avatar,
          content,
          time_str,
          liked_count,
        });
      }
    }

    Ok(result)
  }
}
