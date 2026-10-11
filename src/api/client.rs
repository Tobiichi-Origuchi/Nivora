use ncm_api_rs::{ApiClient, Query, create_client};
use parking_lot::RwLock;
use std::sync::Arc;

use crate::common::error::AppResult;
use crate::data::Database;

use super::artist::ArtistApi;
use super::auth::AuthApi;
use super::comment::CommentApi;
use super::lyric::LyricApi;
use super::playlist::PlaylistApi;
use super::search::SearchApi;
use super::session::SessionManager;
use super::song::SongApi;

pub use crate::common::http::{
  CHINESE_IP_POOL, DEFAULT_USER_AGENT as DESKTOP_USER_AGENT, get_random_disguise_ip,
};

/// 构造请求参数
pub fn create_disguised_query() -> Query {
  let mut q = Query::new();
  q.random_cn_ip = true;
  q.real_ip = Some(get_random_disguise_ip().to_string());
  q.ua = Some(DESKTOP_USER_AGENT.to_string());
  q
}

/// 网络请求重试机制
pub async fn with_retry<F, Fut, T, E>(mut op: F, max_retries: usize) -> Result<T, E>
where
  F: FnMut() -> Fut,
  Fut: std::future::Future<Output = Result<T, E>>,
{
  let mut delay = std::time::Duration::from_millis(200);
  for attempt in 0..max_retries {
    match op().await {
      Ok(val) => return Ok(val),
      Err(_err) if attempt + 1 < max_retries => {
        tokio::time::sleep(delay).await;
        delay = delay.saturating_mul(2);
      }
      Err(err) => return Err(err),
    }
  }
  unreachable!()
}

/// 执行请求
pub async fn request_with_retry<F, Fut, T>(f: F) -> AppResult<T>
where
  F: FnMut() -> Fut,
  Fut: std::future::Future<Output = Result<T, ncm_api_rs::NcmError>>,
{
  with_retry(f, 3)
    .await
    .map_err(crate::common::error::AppError::from)
}

/// 网易云 API 客户端
#[derive(Clone)]
pub struct NcmClient {
  inner: Arc<RwLock<ApiClient>>,
  session: SessionManager,
  auth: AuthApi,
  song: SongApi,
  playlist: PlaylistApi,
  search: SearchApi,
  lyric: LyricApi,
  artist: ArtistApi,
  comment: CommentApi,
}

impl Default for NcmClient {
  fn default() -> Self {
    Self::new(None)
  }
}

impl NcmClient {
  /// 基于指定数据库创建客户端，恢复并管理持久化凭据
  pub fn with_db(db: Database) -> Self {
    let client = Arc::new(RwLock::new(create_client(None)));
    let session = SessionManager::new(db, client.clone());
    let auth = AuthApi::new(session.clone(), client.clone());
    let song = SongApi::new(session.clone(), client.clone());
    let playlist = PlaylistApi::new(session.clone(), client.clone());
    let search = SearchApi::new(session.clone(), client.clone());
    let lyric = LyricApi::new(session.clone(), client.clone());
    let artist = ArtistApi::new(session.clone(), client.clone());
    let comment = CommentApi::new(session.clone(), client.clone());

    Self {
      inner: client,
      session,
      auth,
      song,
      playlist,
      search,
      lyric,
      artist,
      comment,
    }
  }

  /// 创建客户端实例
  pub fn new(cookie: Option<String>) -> Self {
    let db = Database::open_in_memory().expect("初始化内存数据库失败");
    let client = Arc::new(RwLock::new(create_client(cookie.clone())));
    let session = SessionManager::new(db, client.clone());
    if let Some(ref c) = cookie {
      let _ = session.set_cookie(c);
    }
    let auth = AuthApi::new(session.clone(), client.clone());
    let song = SongApi::new(session.clone(), client.clone());
    let playlist = PlaylistApi::new(session.clone(), client.clone());
    let search = SearchApi::new(session.clone(), client.clone());
    let lyric = LyricApi::new(session.clone(), client.clone());
    let artist = ArtistApi::new(session.clone(), client.clone());
    let comment = CommentApi::new(session.clone(), client.clone());

    Self {
      inner: client,
      session,
      auth,
      song,
      playlist,
      search,
      lyric,
      artist,
      comment,
    }
  }

  /// 使用标准数据目录的数据库打开全局客户端
  pub fn open_default() -> AppResult<Self> {
    let db = Database::open_default()?;
    Ok(Self::with_db(db))
  }

  pub fn inner(&self) -> &Arc<RwLock<ApiClient>> {
    &self.inner
  }

  pub fn session(&self) -> &SessionManager {
    &self.session
  }

  pub fn auth(&self) -> &AuthApi {
    &self.auth
  }

  pub fn song(&self) -> &SongApi {
    &self.song
  }

  pub fn playlist(&self) -> &PlaylistApi {
    &self.playlist
  }

  pub fn search(&self) -> &SearchApi {
    &self.search
  }

  pub fn lyric(&self) -> &LyricApi {
    &self.lyric
  }

  pub fn artist(&self) -> &ArtistApi {
    &self.artist
  }

  pub fn comment(&self) -> &CommentApi {
    &self.comment
  }
}
