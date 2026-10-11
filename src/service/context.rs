use parking_lot::{Mutex, RwLock};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use tokio::sync::mpsc;

use crate::api::NcmClient;
use crate::common::channel::{AppEvent, UiAction};
use crate::common::config::AppConfig;
use crate::common::state::AppState;
use crate::data::Database;
use crate::engine::InnerPlayer;
use crate::lyric::LyricTimeline;
use crate::service::playback_svc::PlaybackQueue;
use crate::service::scrobble_svc::ScrobbleService;

/// 全局应用运行时服务与基础设施编排上下文 (解耦 UI，纯后端上下文)
#[derive(Clone)]
pub struct AppContext {
  pub player: Arc<Mutex<InnerPlayer>>,
  pub state: Arc<RwLock<AppState>>,
  pub event_tx: mpsc::UnboundedSender<AppEvent>,
  pub action_tx: mpsc::UnboundedSender<UiAction>,
  pub client: NcmClient,
  pub scrobble_svc: ScrobbleService,
  pub lyric_timeline: Arc<RwLock<LyricTimeline>>,
  pub queue: Arc<Mutex<PlaybackQueue>>,
  pub playback_svc: Arc<Mutex<PlaybackQueue>>,
  pub quality_level: Arc<Mutex<String>>,
  pub db: Database,
  pub config: Arc<Mutex<AppConfig>>,
  /// 当前提取的代表色 [A, R, G, B]，使用原生整型数组解除对 UI 框架类型的强耦合
  pub current_extracted_color: Arc<Mutex<Option<[u8; 4]>>>,
  pub play_token_counter: Arc<AtomicU64>,
  pub active_cancel_token: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}
