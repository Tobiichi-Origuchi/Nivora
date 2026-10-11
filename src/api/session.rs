use ncm_api_rs::ApiClient;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::common::error::AppResult;
use crate::data::{Database, SessionRepo};

const PLATFORM_NETEASE: &str = "netease";

/// 网易云会话管理服务
#[derive(Clone)]
pub struct SessionManager {
  db: Database,
  client: Arc<RwLock<ApiClient>>,
  current_cookie: Arc<RwLock<Option<String>>>,
}

impl SessionManager {
  /// 创建会话管理器并从数据库恢复会话凭据
  pub fn new(db: Database, client: Arc<RwLock<ApiClient>>) -> Self {
    let saved_cookie = SessionRepo::get_cookies(&db, PLATFORM_NETEASE).unwrap_or(None);

    if let Some(ref cookie) = saved_cookie
      && !cookie.trim().is_empty()
    {
      client.write().set_cookie(cookie.clone());
    }

    Self {
      db,
      client,
      current_cookie: Arc::new(RwLock::new(saved_cookie)),
    }
  }

  /// 直接更新/设置完整的 Cookie 字符串并持久化
  pub fn set_cookie(&self, cookie: &str) -> AppResult<()> {
    let cookie_str = cookie.trim().to_string();
    *self.current_cookie.write() = Some(cookie_str.clone());
    self.client.write().set_cookie(cookie_str.clone());
    SessionRepo::save_cookies(&self.db, PLATFORM_NETEASE, &cookie_str)?;
    Ok(())
  }

  /// 增量合并来自 API 响应头或响应体中的 Cookies
  pub fn merge_cookies(&self, set_cookie_list: &[String]) -> AppResult<()> {
    if set_cookie_list.is_empty() {
      return Ok(());
    }

    let mut cookie_map = {
      let current = self.current_cookie.read();
      parse_cookie_map(current.as_deref().unwrap_or(""))
    };

    let mut mutated = false;
    for raw in set_cookie_list {
      // Set-Cookie 格式例如: MUSIC_U=xxx; Max-Age=...; Path=/
      let first_part = raw.split(';').next().unwrap_or("").trim();
      if let Some((k, v)) = first_part.split_once('=') {
        let key = k.trim().to_string();
        let val = v.trim().to_string();
        if !key.is_empty() {
          cookie_map.insert(key, val);
          mutated = true;
        }
      }
    }

    if mutated {
      let merged = cookie_map_to_string(&cookie_map);
      *self.current_cookie.write() = Some(merged.clone());
      self.client.write().set_cookie(merged.clone());
      SessionRepo::save_cookies(&self.db, PLATFORM_NETEASE, &merged)?;
    }

    Ok(())
  }

  /// 清空会话凭据
  pub fn clear_session(&self) -> AppResult<()> {
    *self.current_cookie.write() = None;
    // 清理 Client 中的内部 cookie 字符串
    self.client.write().set_cookie(String::new());
    SessionRepo::clear_session(&self.db, PLATFORM_NETEASE)?;
    Ok(())
  }
}

/// 解析 key=value; 字符串为 HashMap
fn parse_cookie_map(cookie_str: &str) -> HashMap<String, String> {
  let mut map = HashMap::new();
  if cookie_str.trim().is_empty() {
    return map;
  }
  for item in cookie_str.split(';') {
    if let Some((k, v)) = item.trim().split_once('=') {
      map.insert(k.trim().to_string(), v.trim().to_string());
    }
  }
  map
}

/// 将键值映射序列化为标准 Cookie 标头格式
fn cookie_map_to_string(map: &HashMap<String, String>) -> String {
  map
    .iter()
    .map(|(k, v)| format!("{}={}", k, v))
    .collect::<Vec<_>>()
    .join("; ")
}
