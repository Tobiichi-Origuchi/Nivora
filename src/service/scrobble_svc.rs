use parking_lot::Mutex;
use std::sync::Arc;
use tracing::{debug, info};

use crate::api::NcmClient;
use crate::common::error::AppResult;
use crate::common::model::Track;
use crate::common::types::TrackSource;
use crate::data::{Database, HistoryRepo};

/// 单曲播放追踪状态
#[derive(Debug, Clone)]
struct ActivePlaySession {
  track: Track,
  source_id: String,
  last_position_ms: u64,
  total_listened_ms: u64,
  scrobbled: bool,
}

/// 离线待重试打卡记录
#[derive(Debug, Clone)]
struct UnsyncedScrobble {
  track_id: String,
  source_id: String,
  time_secs: i64,
}

/// 听歌打卡与播放足迹编排服务
#[derive(Clone)]
pub struct ScrobbleService {
  client: NcmClient,
  db: Database,
  active_session: Arc<Mutex<Option<ActivePlaySession>>>,
  pending_scrobbles: Arc<Mutex<Vec<UnsyncedScrobble>>>,
}

impl ScrobbleService {
  pub fn new(client: NcmClient, db: Database) -> Self {
    Self {
      client,
      db,
      active_session: Arc::new(Mutex::new(None)),
      pending_scrobbles: Arc::new(Mutex::new(Vec::new())),
    }
  }

  /// 当切换或开始播放新歌曲时触发
  pub fn on_track_started(&self, track: Track, source_id: Option<&str>) {
    let mut session_guard = self.active_session.lock();

    // 若之前已有活跃会话且有有效听歌时长，记录到本地播放足迹
    if let Some(prev) = session_guard.take()
      && prev.total_listened_ms >= 5_000
    {
      let db_c = self.db.clone();
      tokio::task::spawn_blocking(move || {
        if let Err(e) = HistoryRepo::record_play(&db_c, &prev.track, prev.total_listened_ms) {
          tracing::warn!("写入听歌足迹失败 (track: {}): {e}", prev.track.title);
        }
      });
    }

    *session_guard = Some(ActivePlaySession {
      track,
      source_id: source_id.unwrap_or("").to_string(),
      last_position_ms: 0,
      total_listened_ms: 0,
      scrobbled: false,
    });
  }

  /// 随着播放器进度跳动更新监听时长并判断打卡阈值
  pub async fn on_progress_tick(&self, current_pos_ms: u64) -> AppResult<()> {
    let scrobble_request = {
      let mut session_guard = self.active_session.lock();
      let session = match session_guard.as_mut() {
        Some(s) => s,
        None => return Ok(()),
      };

      // 计算增量听歌时长
      let delta = current_pos_ms.saturating_sub(session.last_position_ms);

      if delta > 0 && delta <= 3_000 {
        session.total_listened_ms += delta;
      }
      session.last_position_ms = current_pos_ms;

      // 收听满 50% 或者收听满 30 秒
      let duration_ms = session.track.duration_ms;
      let reaches_half = duration_ms > 0 && session.total_listened_ms * 2 >= duration_ms;
      let reaches_30s = session.total_listened_ms >= 30_000;

      if !session.scrobbled && (reaches_half || reaches_30s) {
        session.scrobbled = true;
        let track = session.track.clone();
        let source_id = session.source_id.clone();
        let time_secs = (session.total_listened_ms / 1000) as i64;
        let total_ms = session.total_listened_ms;
        Some((track, source_id, time_secs, total_ms))
      } else {
        None
      }
    };

    if let Some((track, source_id, time_secs, total_ms)) = scrobble_request {
      // 1. 记录到本地
      let db_c = self.db.clone();
      let tr_c = track.clone();
      let _ =
        tokio::task::spawn_blocking(move || HistoryRepo::record_play(&db_c, &tr_c, total_ms)).await;

      // 2. 若为网易云曲目且已登录，调用打卡接口
      if track.source == TrackSource::Ncm {
        info!(
          "单曲 [{}] 播放达标，触发网易云打卡 (时长 {} 秒)",
          track.title, time_secs
        );
        let client = self.client.clone();
        let track_id = track.id.clone();
        let pending = self.pending_scrobbles.clone();
        let src_id = source_id.clone();
        tokio::spawn(async move {
          if let Err(err) = client.song().scrobble(&track_id, &src_id, time_secs).await {
            debug!("网易云打卡接口返回提示，加入待重试队列: {err}");
            let mut guard = pending.lock();
            guard.push(UnsyncedScrobble {
              track_id,
              source_id: src_id,
              time_secs,
            });
          }
        });
      }
    }

    Ok(())
  }

  /// 重试同步离线或失败的打卡请求
  pub async fn flush_unsynced_scrobbles(&self) -> usize {
    let items: Vec<UnsyncedScrobble> = {
      let mut guard = self.pending_scrobbles.lock();
      std::mem::take(&mut *guard)
    };

    if items.is_empty() {
      return 0;
    }

    let mut failed = Vec::new();
    let mut succeeded = 0;

    for item in items {
      match self
        .client
        .song()
        .scrobble(&item.track_id, &item.source_id, item.time_secs)
        .await
      {
        Ok(_) => {
          succeeded += 1;
        }
        Err(err) => {
          debug!("离线打卡重试失败 [track_id={}]: {err}", item.track_id);
          failed.push(item);
        }
      }
    }

    if !failed.is_empty() {
      let mut guard = self.pending_scrobbles.lock();
      guard.extend(failed);
    }

    info!(
      "离线打卡同步完成: 成功 {} 条, 剩余 {} 条待同步",
      succeeded,
      self.pending_scrobbles.lock().len()
    );
    succeeded
  }

  /// 歌曲播放完毕或主动停止时触发收尾
  pub fn on_track_ended(&self) {
    let mut session_guard = self.active_session.lock();
    if let Some(prev) = session_guard.take()
      && prev.total_listened_ms >= 5_000
    {
      let db_c = self.db.clone();
      tokio::task::spawn_blocking(move || {
        if let Err(e) = HistoryRepo::record_play(&db_c, &prev.track, prev.total_listened_ms) {
          tracing::warn!("写入听歌足迹失败 (track: {}): {e}", prev.track.title);
        }
      });
    }
  }
}
