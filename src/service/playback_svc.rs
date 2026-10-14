use rand::seq::SliceRandom;
use rand::thread_rng;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::info;

use crate::common::channel::{AppEvent, UiAction};
use crate::common::model::Track;
use crate::common::types::{AudioQualityLevel, PlayMode, PlayerState, TrackSource};
use crate::data::repo_history::HistoryRepo;
use crate::lyric::LyricTimeline;
use crate::service::cover_svc::CoverService;
use crate::service::library_svc::LibraryService;
use crate::service::lyric_svc::LyricService;
use crate::service::playlist_svc::PlaylistService;

pub use crate::service::context::AppContext;

/// 纯内存播放队列与调度状态机
#[derive(Debug, Clone)]
pub struct PlaybackQueue {
  queue: Vec<Track>,
  current_index: Option<usize>,
  play_mode: PlayMode,
  shuffle_indices: Vec<usize>,
  shuffle_pos: usize,
}

impl Default for PlaybackQueue {
  fn default() -> Self {
    Self::new()
  }
}

impl PlaybackQueue {
  pub fn new() -> Self {
    Self {
      queue: Vec::new(),
      current_index: None,
      play_mode: PlayMode::Sequence,
      shuffle_indices: Vec::new(),
      shuffle_pos: 0,
    }
  }

  pub fn with_mode(play_mode: PlayMode) -> Self {
    Self {
      queue: Vec::new(),
      current_index: None,
      play_mode,
      shuffle_indices: Vec::new(),
      shuffle_pos: 0,
    }
  }

  /// 获取当前队列曲目列表
  pub fn queue(&self) -> &[Track] {
    &self.queue
  }

  /// 获取当前播放曲目索引
  pub fn current_index(&self) -> Option<usize> {
    self.current_index
  }

  /// 获取当前正在播放的曲目
  pub fn current_track(&self) -> Option<&Track> {
    self.current_index.and_then(|idx| self.queue.get(idx))
  }

  /// 获取当前播放模式
  pub fn play_mode(&self) -> PlayMode {
    self.play_mode
  }

  /// 切换循环播放模式
  pub fn set_play_mode(&mut self, mode: PlayMode) {
    self.play_mode = mode;
    if mode == PlayMode::Shuffle {
      self.regenerate_shuffle(self.current_index);
    }
  }

  /// 播放全部
  pub fn play_all_from_list(&mut self, tracks: Vec<Track>, start_index: usize) -> Option<&Track> {
    if tracks.is_empty() {
      self.clear_queue();
      return None;
    }

    let safe_index = start_index.min(tracks.len() - 1);
    self.queue = tracks;
    self.current_index = Some(safe_index);

    if self.play_mode == PlayMode::Shuffle {
      self.regenerate_shuffle(Some(safe_index));
    }

    self.current_track()
  }

  /// 播放指定曲目
  pub fn play_track(&mut self, track: Track) -> Option<&Track> {
    if let Some(pos) = self.queue.iter().position(|t| t.id == track.id) {
      self.current_index = Some(pos);
      if self.play_mode == PlayMode::Shuffle {
        self.sync_shuffle_pos_to_current();
      }
    } else {
      let next_idx = self
        .current_index
        .map(|i| i + 1)
        .unwrap_or(0)
        .min(self.queue.len());
      self.queue.insert(next_idx, track);
      self.current_index = Some(next_idx);
      if self.play_mode == PlayMode::Shuffle {
        self.regenerate_shuffle(Some(next_idx));
      }
    }
    self.current_track()
  }

  /// 添加曲目至播放队列尾部
  pub fn add_to_queue(&mut self, track: Track) {
    if !self.queue.iter().any(|t| t.id == track.id) {
      self.queue.push(track);
      if self.current_index.is_none() {
        self.current_index = Some(0);
      }
      if self.play_mode == PlayMode::Shuffle {
        self.regenerate_shuffle(self.current_index);
      }
    }
  }

  /// 将曲目添加为下一首播放
  pub fn add_to_next(&mut self, track: Track) {
    if let Some(existing) = self.queue.iter().position(|t| t.id == track.id) {
      let item = self.queue.remove(existing);
      let target = self
        .current_index
        .map(|i| i + 1)
        .unwrap_or(0)
        .min(self.queue.len());
      self.queue.insert(target, item);
    } else {
      let target = self
        .current_index
        .map(|i| i + 1)
        .unwrap_or(0)
        .min(self.queue.len());
      self.queue.insert(target, track);
    }
    if self.play_mode == PlayMode::Shuffle {
      self.regenerate_shuffle(self.current_index);
    }
  }

  /// 从队列中移除指定索引的曲目
  pub fn remove_from_queue(&mut self, index: usize) -> Option<Track> {
    if index >= self.queue.len() {
      return None;
    }

    let removed = self.queue.remove(index);
    if self.queue.is_empty() {
      self.current_index = None;
      self.shuffle_indices.clear();
      self.shuffle_pos = 0;
      return Some(removed);
    }

    if let Some(cur) = self.current_index {
      if index < cur {
        self.current_index = Some(cur - 1);
      } else if index == cur {
        self.current_index = Some(cur.min(self.queue.len() - 1));
      }
    }

    if self.play_mode == PlayMode::Shuffle {
      self.regenerate_shuffle(self.current_index);
    }

    Some(removed)
  }

  /// 清空播放队列
  pub fn clear_queue(&mut self) {
    self.queue.clear();
    self.current_index = None;
    self.shuffle_indices.clear();
    self.shuffle_pos = 0;
  }

  /// 设置并覆盖当前播放队列
  pub fn set_queue(&mut self, queue: Vec<Track>) {
    self.queue = queue;
    self.current_index = None;
    self.shuffle_indices.clear();
    self.shuffle_pos = 0;
  }

  /// 切换至下一曲
  pub fn next_track(&mut self) -> Option<&Track> {
    if self.queue.is_empty() {
      return None;
    }

    match self.play_mode {
      PlayMode::RepeatOne => self.current_track(),
      PlayMode::Shuffle => {
        if self.shuffle_indices.is_empty() {
          self.regenerate_shuffle(self.current_index);
        }
        self.shuffle_pos = (self.shuffle_pos + 1) % self.shuffle_indices.len();
        let next_idx = self.shuffle_indices[self.shuffle_pos];
        self.current_index = Some(next_idx);
        self.current_track()
      }
      PlayMode::Sequence => {
        let cur = self.current_index.unwrap_or(0);
        if cur + 1 < self.queue.len() {
          self.current_index = Some(cur + 1);
          self.current_track()
        } else {
          None
        }
      }
      PlayMode::RepeatAll => {
        let cur = self.current_index.unwrap_or(0);
        self.current_index = Some((cur + 1) % self.queue.len());
        self.current_track()
      }
    }
  }

  /// 切换至上一曲
  pub fn prev_track(&mut self) -> Option<&Track> {
    if self.queue.is_empty() {
      return None;
    }

    match self.play_mode {
      PlayMode::RepeatOne => self.current_track(),
      PlayMode::Shuffle => {
        if self.shuffle_indices.is_empty() {
          self.regenerate_shuffle(self.current_index);
        }
        if self.shuffle_pos == 0 {
          self.shuffle_pos = self.shuffle_indices.len().saturating_sub(1);
        } else {
          self.shuffle_pos -= 1;
        }
        let prev_idx = self.shuffle_indices[self.shuffle_pos];
        self.current_index = Some(prev_idx);
        self.current_track()
      }
      PlayMode::Sequence => {
        let cur = self.current_index.unwrap_or(0);
        if cur > 0 {
          self.current_index = Some(cur - 1);
          self.current_track()
        } else {
          None
        }
      }
      PlayMode::RepeatAll => {
        let cur = self.current_index.unwrap_or(0);
        let prev = if cur == 0 {
          self.queue.len().saturating_sub(1)
        } else {
          cur - 1
        };
        self.current_index = Some(prev);
        self.current_track()
      }
    }
  }

  fn regenerate_shuffle(&mut self, current: Option<usize>) {
    let count = self.queue.len();
    if count == 0 {
      self.shuffle_indices.clear();
      self.shuffle_pos = 0;
      return;
    }

    let mut indices: Vec<usize> = (0..count).collect();
    let mut rng = thread_rng();
    indices.shuffle(&mut rng);

    if let Some(cur) = current {
      if let Some(pos) = indices.iter().position(|&x| x == cur) {
        indices.swap(0, pos);
      }
      self.shuffle_pos = 0;
    }

    self.shuffle_indices = indices;
  }

  fn sync_shuffle_pos_to_current(&mut self) {
    if let Some(cur) = self.current_index
      && let Some(pos) = self.shuffle_indices.iter().position(|&x| x == cur)
    {
      self.shuffle_pos = pos;
    }
  }
}

/// 播放业务编排与控制调度服务
pub struct PlaybackService;

impl PlaybackService {
  /// 播放领域动作统一调度入口
  pub async fn dispatch(ctx: &AppContext, action: UiAction) {
    match action {
      UiAction::TogglePlayPause => Self::toggle_play_pause(ctx),
      UiAction::PlayTrack(id) => Self::play_track_flow(ctx, id).await,
      UiAction::PrevTrack => Self::prev_track_flow(ctx),
      UiAction::NextTrack => Self::next_track_flow(ctx),
      UiAction::PlayAllFromList(id) => Self::play_all_from_list_flow(ctx, id),
      UiAction::PlayRandomTrack => Self::play_random_track_flow(ctx),
      UiAction::ClearQueue => Self::clear_queue_flow(ctx),
      UiAction::RemoveQueueTrack(idx) => Self::remove_queue_track_flow(ctx, idx),
      UiAction::SetVolume(vol) => Self::set_volume(ctx, vol),
      UiAction::ToggleMute => Self::toggle_mute(ctx),
      UiAction::SetPlayMode(mode) => Self::set_play_mode_flow(ctx, mode),
      UiAction::CycleQuality => Self::cycle_quality_flow(ctx),
      UiAction::SelectQuality(q) => Self::select_quality_flow(ctx, &q),
      UiAction::SeekTo(pos_ms) => Self::seek_to(ctx, pos_ms),
      UiAction::SetEqGain(band, gain) => Self::set_eq_gain_flow(ctx, band, gain),
      UiAction::SetEqPreset(preset) => Self::set_eq_preset_flow(ctx, preset),
      _ => {}
    }
  }

  /// 播放/暂停切换
  pub fn toggle_play_pause(ctx: &AppContext) {
    let mut p = ctx.player.lock();
    let current_state = p.state();
    if current_state == PlayerState::Playing {
      let _ = p.pause();
      ctx.state.write().player_state = PlayerState::Paused;
      let _ = ctx
        .event_tx
        .send(AppEvent::PlayerStateChanged(PlayerState::Paused));
    } else {
      let _ = p.play();
      ctx.state.write().player_state = PlayerState::Playing;
      let _ = ctx
        .event_tx
        .send(AppEvent::PlayerStateChanged(PlayerState::Playing));
    }
  }

  /// 进度跳转
  pub fn seek_to(ctx: &AppContext, pos_ms: u64) {
    let mut p = ctx.player.lock();
    let _ = p.seek(pos_ms);
    let dur = p.duration_ms();
    let _ = ctx.event_tx.send(AppEvent::ProgressUpdated {
      position_ms: pos_ms,
      duration_ms: dur,
    });
  }

  /// 设置音量
  pub fn set_volume(ctx: &AppContext, vol: f32) {
    ctx.player.lock().set_volume(vol);
    {
      let mut state = ctx.state.write();
      state.volume = vol;
      state.volume_before_mute = None;
    }
    if let Err(e) = crate::data::SettingsRepo::set(&ctx.db, "volume", &vol.to_string()) {
      tracing::warn!("持久化音量设置失败: {e}");
    }
    ctx.config.lock().volume = vol;
    let _ = ctx.event_tx.send(AppEvent::VolumeChanged(vol));
  }

  /// 静音切换。
  pub fn toggle_mute(ctx: &AppContext) {
    let (current_vol, muted) = {
      let state = ctx.state.read();
      (state.volume, state.volume_before_mute.is_some())
    };
    if !muted && current_vol > 0.0 {
      ctx.state.write().volume_before_mute = Some(current_vol);
      ctx.player.lock().set_volume(0.0);
      let _ = ctx.event_tx.send(AppEvent::VolumeChanged(0.0));
    } else {
      let restore = {
        let mut state = ctx.state.write();
        let saved = state.volume_before_mute.take().unwrap_or(state.volume);
        if saved > 0.0 { saved } else { 0.8 }
      };
      Self::set_volume(ctx, restore);
    }
  }

  /// 切换循环播放模式
  pub fn set_play_mode_flow(ctx: &AppContext, mode: PlayMode) {
    ctx.playback_svc.lock().set_play_mode(mode);
    ctx.state.write().play_mode = mode;
    if let Err(e) = crate::data::SettingsRepo::set_json(&ctx.db, "play_mode", &mode) {
      tracing::warn!("持久化播放模式失败: {e}");
    }
    ctx.config.lock().play_mode = mode;
    info!("已持久化保存播放模式到数据库: {:?}", mode);
    let _ = ctx.event_tx.send(AppEvent::PlayModeChanged(mode));
  }

  /// 轮换音质等级
  pub fn cycle_quality_flow(ctx: &AppContext) {
    let mut q = ctx.quality_level.lock();
    let (next_q, badge, quality_enum) = match q.as_str() {
      "standard" => ("higher", "较高", AudioQualityLevel::Higher),
      "higher" => ("exhigh", "极高", AudioQualityLevel::ExHigh),
      "exhigh" => ("lossless", "SQ", AudioQualityLevel::Lossless),
      "lossless" => ("hires", "Hi-Res", AudioQualityLevel::HiRes),
      _ => ("standard", "标准", AudioQualityLevel::Standard),
    };
    *q = next_q.to_string();
    if let Err(e) = crate::data::SettingsRepo::set_json(&ctx.db, "audio_quality", &quality_enum) {
      tracing::warn!("持久化播放音质失败: {e}");
    }
    ctx.config.lock().audio_quality = quality_enum;
    info!("切换并持久化保存播放音质到数据库: {next_q} ({badge})");
    let _ = ctx
      .event_tx
      .send(AppEvent::QualityChanged(badge.to_string()));
  }

  /// 选择指定音质等级
  pub fn select_quality_flow(ctx: &AppContext, quality_name: &str) {
    let (badge, quality_enum) = match quality_name {
      "standard" | "标准" => ("标准", AudioQualityLevel::Standard),
      "higher" | "较高" => ("较高", AudioQualityLevel::Higher),
      "exhigh" | "极高" => ("极高", AudioQualityLevel::ExHigh),
      "lossless" | "无损" | "SQ" => ("SQ", AudioQualityLevel::Lossless),
      "hires" | "Hi-Res" => ("Hi-Res", AudioQualityLevel::HiRes),
      _ => ("标准", AudioQualityLevel::Standard),
    };
    *ctx.quality_level.lock() = quality_enum.as_level_str().to_string();
    if let Err(e) = crate::data::SettingsRepo::set_json(&ctx.db, "audio_quality", &quality_enum) {
      tracing::warn!("持久化播放音质选择失败: {e}");
    }
    ctx.config.lock().audio_quality = quality_enum;
    info!("选择并持久化保存播放音质到数据库: {badge}");
    let _ = ctx
      .event_tx
      .send(AppEvent::QualityChanged(badge.to_string()));
  }

  /// 清空播放队列
  pub fn clear_queue_flow(ctx: &AppContext) {
    ctx.playback_svc.lock().clear_queue();
    if let Err(e) =
      crate::data::SettingsRepo::set_json(&ctx.db, "playback_queue", &Vec::<Track>::new())
    {
      tracing::warn!("清空持久化播放队列失败: {e}");
    }
    let _ = ctx.event_tx.send(AppEvent::QueueUpdated(Vec::new()));
  }

  /// 从播放队列中移除曲目
  pub fn remove_queue_track_flow(ctx: &AppContext, idx: usize) {
    ctx.playback_svc.lock().remove_from_queue(idx);
    let q = ctx.playback_svc.lock().queue().to_vec();
    if let Err(e) = crate::data::SettingsRepo::set_json(&ctx.db, "playback_queue", &q) {
      tracing::warn!("持久化更新播放队列失败: {e}");
    }
    let _ = ctx.event_tx.send(AppEvent::QueueUpdated(q));
  }

  /// 设置均衡器增益
  pub fn set_eq_gain_flow(ctx: &AppContext, band: usize, gain: f32) {
    ctx.player.lock().set_eq_gain(band, gain);
    let gains = ctx.player.lock().equalizer_params().load_gains().to_vec();
    // TODO: "自定义" 改为 Option<EqPreset>::None。
    let preset_name = crate::engine::CUSTOM_EQ_PRESET_NAME.to_string();
    {
      let mut cfg = ctx.config.lock();
      cfg.eq_gains = gains.clone();
      cfg.eq_preset = preset_name.clone();
    }
    if let Err(e) = crate::data::SettingsRepo::set_json(&ctx.db, "eq_gains", &gains) {
      tracing::warn!("持久化保存均衡器增益失败: {e}");
    }
    if let Err(e) = crate::data::SettingsRepo::set(&ctx.db, "eq_preset", &preset_name) {
      tracing::warn!("持久化保存均衡器预设失败: {e}");
    }
    let _ = ctx
      .event_tx
      .send(AppEvent::EqualizerChanged { preset_name, gains });
  }

  /// 设置均衡器预设
  pub fn set_eq_preset_flow(ctx: &AppContext, preset: String) {
    ctx.player.lock().set_eq_preset(&preset);
    let gains = ctx.player.lock().equalizer_params().load_gains().to_vec();
    if let Err(e) = crate::data::SettingsRepo::set(&ctx.db, "eq_preset", &preset) {
      tracing::warn!("持久化保存均衡器预设失败: {e}");
    }
    if let Err(e) = crate::data::SettingsRepo::set_json(&ctx.db, "eq_gains", &gains) {
      tracing::warn!("持久化保存均衡器增益失败: {e}");
    }
    {
      let mut cfg = ctx.config.lock();
      cfg.eq_preset = preset.clone();
      cfg.eq_gains = gains.clone();
    }
    let _ = ctx.event_tx.send(AppEvent::EqualizerChanged {
      preset_name: preset,
      gains,
    });
  }

  /// 切换至上一曲编排
  pub fn prev_track_flow(ctx: &AppContext) {
    let prev_opt = ctx.playback_svc.lock().prev_track().cloned();
    if let Some(track) = prev_opt {
      ctx.state.write().current_track = Some(track.clone());
      let is_liked = ctx
        .state
        .read()
        .liked_songs
        .iter()
        .any(|t| t.id == track.id);
      let _ = ctx.event_tx.send(AppEvent::TrackLikeStateChanged(is_liked));
      let _ = ctx
        .event_tx
        .send(AppEvent::TrackChanged(Some(track.clone())));
      let _ = ctx.action_tx.send(UiAction::PlayTrack(track.id));
    }
  }

  /// 切换至下一曲编排
  pub fn next_track_flow(ctx: &AppContext) {
    let next_opt = ctx.playback_svc.lock().next_track().cloned();
    if let Some(track) = next_opt {
      ctx.state.write().current_track = Some(track.clone());
      let is_liked = ctx
        .state
        .read()
        .liked_songs
        .iter()
        .any(|t| t.id == track.id);
      let _ = ctx.event_tx.send(AppEvent::TrackLikeStateChanged(is_liked));
      let _ = ctx
        .event_tx
        .send(AppEvent::TrackChanged(Some(track.clone())));
      let _ = ctx.action_tx.send(UiAction::PlayTrack(track.id));
    }
  }

  /// 一键播放指定列表中的全部歌曲编排
  pub fn play_all_from_list_flow(ctx: &AppContext, list_id: String) {
    info!("一键播放全部歌曲: {list_id}");
    let client = ctx.client.clone();
    let db = ctx.db.clone();
    let tx_action = ctx.action_tx.clone();
    let p_svc = ctx.playback_svc.clone();
    let ev_tx = ctx.event_tx.clone();
    let st = ctx.state.clone();

    if list_id == "daily" {
      let tracks = st.read().daily_songs.clone();
      if let Some(first) = p_svc.lock().play_all_from_list(tracks.clone(), 0).cloned() {
        if let Err(e) = crate::data::SettingsRepo::set_json(&db, "playback_queue", &tracks) {
          tracing::warn!("持久化播放队列失败: {e}");
        }
        let _ = ev_tx.send(AppEvent::QueueUpdated(tracks));
        let _ = tx_action.send(UiAction::PlayTrack(first.id));
      }
    } else if list_id == "liked" {
      let db_c = db.clone();
      tokio::spawn(async move {
        let db_query = db_c.clone();
        if let Ok(Ok(favs)) =
          tokio::task::spawn_blocking(move || HistoryRepo::list_favorites(&db_query, 1000)).await
        {
          let tracks: Vec<Track> = favs.into_iter().map(|f| f.track).collect();
          if let Some(first) = p_svc.lock().play_all_from_list(tracks.clone(), 0).cloned() {
            if let Err(e) = crate::data::SettingsRepo::set_json(&db_c, "playback_queue", &tracks) {
              tracing::warn!("持久化播放队列失败: {e}");
            }
            let _ = ev_tx.send(AppEvent::QueueUpdated(tracks));
            let _ = tx_action.send(UiAction::PlayTrack(first.id));
          }
        }
      });
    } else if list_id == "artist_hot" {
      let tracks = st.read().artist_hot_songs.clone();
      if let Some(first) = p_svc.lock().play_all_from_list(tracks.clone(), 0).cloned() {
        if let Err(e) = crate::data::SettingsRepo::set_json(&db, "playback_queue", &tracks) {
          tracing::warn!("持久化播放队列失败: {e}");
        }
        let _ = ev_tx.send(AppEvent::QueueUpdated(tracks));
        let _ = tx_action.send(UiAction::PlayTrack(first.id));
      }
    } else if list_id == "local" {
      let db_c = db.clone();
      tokio::spawn(async move {
        let db_query = db_c.clone();
        if let Ok(Ok(tracks)) =
          tokio::task::spawn_blocking(move || LibraryService::get_all_local_tracks(&db_query)).await
          && let Some(first) = p_svc.lock().play_all_from_list(tracks.clone(), 0).cloned()
        {
          if let Err(e) = crate::data::SettingsRepo::set_json(&db_c, "playback_queue", &tracks) {
            tracing::warn!("持久化播放队列失败: {e}");
          }
          let _ = ev_tx.send(AppEvent::QueueUpdated(tracks));
          let _ = tx_action.send(UiAction::PlayTrack(first.id));
        }
      });
    } else if list_id == "fm" {
      let client = client.clone();
      let p_svc = p_svc.clone();
      let db = db.clone();
      let ev_tx = ev_tx.clone();
      let tx_action = tx_action.clone();
      let st = st.clone();
      tokio::spawn(async move {
        let tracks = match client.song().get_personal_fm().await {
          Ok(t) if !t.is_empty() => t,
          _ => st.read().daily_songs.clone(),
        };
        if !tracks.is_empty() {
          if let Some(first) = p_svc.lock().play_all_from_list(tracks.clone(), 0).cloned() {
            if let Err(e) = crate::data::SettingsRepo::set_json(&db, "playback_queue", &tracks) {
              tracing::warn!("持久化播放队列失败: {e}");
            }
            let _ = ev_tx.send(AppEvent::QueueUpdated(tracks));
            let _ = tx_action.send(UiAction::PlayTrack(first.id));
            let _ = ev_tx.send(AppEvent::ShowToast(
              "已为您开启私人 FM 推荐电台".to_string(),
            ));
          }
        } else {
          let _ = tx_action.send(UiAction::PlayRandomTrack);
          let _ = ev_tx.send(AppEvent::ShowToast("已开启私人推荐模式".to_string()));
        }
      });
    } else {
      let db_c = db.clone();
      tokio::spawn(async move {
        if let Ok(tracks) = client.playlist().get_playlist_tracks(&list_id, 0).await
          && let Some(first) = p_svc.lock().play_all_from_list(tracks.clone(), 0).cloned()
        {
          if let Err(e) = crate::data::SettingsRepo::set_json(&db_c, "playback_queue", &tracks) {
            tracing::warn!("持久化播放队列失败: {e}");
          }
          let _ = ev_tx.send(AppEvent::QueueUpdated(tracks));
          let _ = tx_action.send(UiAction::PlayTrack(first.id));
        }
      });
    }
  }

  /// 随机播放推荐曲目编排
  pub fn play_random_track_flow(ctx: &AppContext) {
    let st = ctx.state.read();
    let mut candidates = st.daily_songs.clone();
    if candidates.is_empty() {
      candidates = st.queue.clone();
    }
    if candidates.is_empty() {
      candidates = st.liked_songs.clone();
    }
    if candidates.is_empty() {
      candidates = st.artist_hot_songs.clone();
    }
    drop(st);

    if !candidates.is_empty() {
      let idx = rand::random::<usize>() % candidates.len();
      let selected = candidates[idx].clone();
      info!(
        "试试手气随机抽中曲目: {} (ID: {})",
        selected.title, selected.id
      );
      let _ = ctx.action_tx.send(UiAction::PlayTrack(selected.id));
    }
  }

  /// 播放指定曲目 (本地 / 网易云流媒体)
  pub async fn play_track_flow(ctx: &AppContext, track_id: String) {
    info!("开始加载并播放曲目: {track_id}");
    ctx.lyric_timeline.write().set_offset_ms(0);
    let play_token = ctx.play_token_counter.fetch_add(1, Ordering::SeqCst) + 1;
    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
      let mut prev_token = ctx.active_cancel_token.lock();
      if let Some(prev) = prev_token.take() {
        prev.store(true, Ordering::Relaxed);
      }
      *prev_token = Some(cancel_flag.clone());
    }

    let client = ctx.client.clone();
    let p = ctx.player.clone();
    let ev_tx = ctx.event_tx.clone();
    let st = ctx.state.clone();
    let scrobble = ctx.scrobble_svc.clone();
    let lyric_timeline_worker = ctx.lyric_timeline.clone();
    let p_svc = ctx.playback_svc.clone();
    let q_level = ctx.quality_level.lock().clone();
    let db = ctx.db.clone();
    let ext_color = ctx.current_extracted_color.clone();
    let pt_counter = ctx.play_token_counter.clone();

    if track_id.starts_with("local:") {
      let path_str = track_id.trim_start_matches("local:").to_string();
      let path = PathBuf::from(&path_str);

      let lrc_path = path.with_extension("lrc");
      if lrc_path.exists() {
        if let Ok(lrc_content) = std::fs::read_to_string(&lrc_path) {
          let lines = crate::lyric::parse_lyrics_bundle(Some(&lrc_content), None, None, None);
          *lyric_timeline_worker.write() = LyricTimeline::new(lines.clone());
          let _ = ev_tx.send(AppEvent::LyricsLoaded(lines));
        }
      } else {
        *lyric_timeline_worker.write() = LyricTimeline::new(Vec::new());
        let _ = ev_tx.send(AppEvent::LyricsLoaded(Vec::new()));
      }

      let track_meta = LibraryService::get_all_local_tracks(&db)
        .ok()
        .and_then(|list| list.into_iter().find(|t| t.id == track_id))
        .unwrap_or_else(|| Track {
          id: track_id.clone(),
          title: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("本地音频")
            .to_string(),
          artists: vec![],
          album: None,
          duration_ms: 0,
          cover_url: None,
          cover_original: None,
          source: TrackSource::Local,
          local_path: Some(path_str.clone()),
          fee: 0,
        });

      p_svc.lock().play_track(track_meta.clone());
      st.write().current_track = Some(track_meta.clone());
      let is_liked = st.read().liked_songs.iter().any(|t| t.id == track_meta.id);
      let _ = ev_tx.send(AppEvent::TrackLikeStateChanged(is_liked));
      let _ = ev_tx.send(AppEvent::TrackChanged(Some(track_meta.clone())));
      let _ = ev_tx.send(AppEvent::QueueUpdated(p_svc.lock().queue().to_vec()));

      let cover_target = track_meta
        .cover_original
        .clone()
        .or_else(|| track_meta.cover_url.clone())
        .or_else(|| track_meta.local_path.clone());
      if let Some(target) = cover_target {
        let ev_tx_c = ev_tx.clone();
        let key = track_id.clone();
        let ext_c = ext_color.clone();
        let std = st.clone();
        let pt_cover = pt_counter.clone();
        let token_cover = play_token;
        tokio::spawn(async move {
          if let Some((path, accent_color)) = CoverService::load_cover(&target, &key).await {
            // TODO: play_token 比较收成一个代数守卫。
            if pt_cover.load(Ordering::SeqCst) != token_cover {
              return;
            }
            *ext_c.lock() = Some([
              accent_color.alpha(),
              accent_color.red(),
              accent_color.green(),
              accent_color.blue(),
            ]);
            let _ = ev_tx_c.send(AppEvent::CoverLoaded { path, accent_color });
            if std.read().dynamic_color_enabled {
              let _ = ev_tx_c.send(AppEvent::ThemeColorChanged(accent_color));
            }
          }
        });
      }

      match crate::engine::open_file_source(&path) {
        Ok(source) => {
          let p_load = p.clone();
          let dur = tokio::task::spawn_blocking(move || {
            let mut player = p_load.lock();
            player.load_source(source, true).unwrap_or(0)
          })
          .await
          .unwrap_or(0);
          st.write().duration_ms = dur;
          st.write().player_state = PlayerState::Playing;
          scrobble.on_track_started(track_meta.clone(), None);
          let _ = ev_tx.send(AppEvent::PlayerStateChanged(PlayerState::Playing));
          let _ = ev_tx.send(AppEvent::ProgressUpdated {
            position_ms: 0,
            duration_ms: dur,
          });
          PlaylistService::refresh_stats(&db, &ev_tx);
        }
        Err(e) => {
          tracing::error!("打开本地音频失败: {e}");
          let _ = ev_tx.send(AppEvent::PlayerStateChanged(PlayerState::Error));
        }
      }
    } else {
      let lyric_track_id = track_id.clone();
      let client_lyric = client.clone();
      let ev_tx_lyric = ev_tx.clone();
      let db_lyric = db.clone();
      let pt_lyric = pt_counter.clone();
      let token_lyric = play_token;

      tokio::spawn(async move {
        let (lrc, yrc, trans) =
          LyricService::load_lyrics(&client_lyric, &db_lyric, &lyric_track_id).await;
        if lrc.is_some() || yrc.is_some() || trans.is_some() {
          let lines = crate::lyric::parse_lyrics_bundle(
            lrc.as_deref(),
            yrc.as_deref(),
            trans.as_deref(),
            None,
          );
          // TODO: play_token 比较收成一个代数守卫。
          if pt_lyric.load(Ordering::SeqCst) != token_lyric {
            return;
          }
          *lyric_timeline_worker.write() = LyricTimeline::new(lines.clone());
          let _ = ev_tx_lyric.send(AppEvent::LyricsLoaded(lines));
        }
      });

      let cur_meta = p_svc
        .lock()
        .queue()
        .iter()
        .find(|t| t.id == track_id)
        .cloned();
      let known_track = cur_meta.clone();
      let detail_rx = if let Some(meta) = cur_meta {
        p_svc.lock().play_track(meta.clone());
        st.write().current_track = Some(meta.clone());
        let is_liked = st.read().liked_songs.iter().any(|t| t.id == meta.id);
        let _ = ev_tx.send(AppEvent::TrackLikeStateChanged(is_liked));
        let _ = ev_tx.send(AppEvent::TrackChanged(Some(meta.clone())));
        let _ = ev_tx.send(AppEvent::QueueUpdated(p_svc.lock().queue().to_vec()));
        let cover = meta
          .cover_original
          .clone()
          .or_else(|| meta.cover_url.clone());
        if let Some(cover) = cover {
          let ev_tx_c = ev_tx.clone();
          let key = track_id.clone();
          let ext_c = ext_color.clone();
          let std = st.clone();
          let pt_cover = pt_counter.clone();
          let token_cover = play_token;
          tokio::spawn(async move {
            if let Some((path, accent_color)) = CoverService::load_cover(&cover, &key).await {
              if pt_cover.load(Ordering::SeqCst) != token_cover {
                return;
              }
              *ext_c.lock() = Some([
                accent_color.alpha(),
                accent_color.red(),
                accent_color.green(),
                accent_color.blue(),
              ]);
              let _ = ev_tx_c.send(AppEvent::CoverLoaded { path, accent_color });
              if std.read().dynamic_color_enabled {
                let _ = ev_tx_c.send(AppEvent::ThemeColorChanged(accent_color));
              }
            }
          });
        }
        None
      } else {
        let client_detail = client.clone();
        let track_id_detail = track_id.clone();
        let ev_tx_detail = ev_tx.clone();
        let p_svc_detail = p_svc.clone();
        let ext_c = ext_color.clone();
        let std = st.clone();
        let pt_detail = pt_counter.clone();
        let token_detail = play_token;
        let (track_tx, track_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
          if let Ok(details) = client_detail
            .song()
            .get_song_detail(&[&track_id_detail])
            .await
            && let Some(track) = details.into_iter().next()
          {
            if pt_detail.load(Ordering::SeqCst) != token_detail {
              return;
            }
            let _ = track_tx.send(track.clone());
            p_svc_detail.lock().play_track(track.clone());
            std.write().current_track = Some(track.clone());
            let is_liked = std.read().liked_songs.iter().any(|t| t.id == track.id);
            let _ = ev_tx_detail.send(AppEvent::TrackLikeStateChanged(is_liked));
            let _ = ev_tx_detail.send(AppEvent::TrackChanged(Some(track.clone())));
            let _ = ev_tx_detail.send(AppEvent::QueueUpdated(p_svc_detail.lock().queue().to_vec()));
            let cover = track
              .cover_original
              .clone()
              .or_else(|| track.cover_url.clone());
            if let Some(cover) = cover {
              let ev_tx_sub = ev_tx_detail.clone();
              let key = track_id_detail.clone();
              let ext_sub = ext_c.clone();
              let std_sub = std.clone();
              let pt_cover = pt_detail.clone();
              let token_cover = token_detail;
              tokio::spawn(async move {
                if let Some((path, accent_color)) = CoverService::load_cover(&cover, &key).await {
                  if pt_cover.load(Ordering::SeqCst) != token_cover {
                    return;
                  }
                  *ext_sub.lock() = Some([
                    accent_color.alpha(),
                    accent_color.red(),
                    accent_color.green(),
                    accent_color.blue(),
                  ]);
                  let _ = ev_tx_sub.send(AppEvent::CoverLoaded { path, accent_color });
                  if std_sub.read().dynamic_color_enabled {
                    let _ = ev_tx_sub.send(AppEvent::ThemeColorChanged(accent_color));
                  }
                }
              });
            }
          }
        });
        Some(track_rx)
      };

      let cancel_flag_task = cancel_flag.clone();
      tokio::spawn(async move {
        if pt_counter.load(Ordering::SeqCst) != play_token {
          return;
        }
        let _ = ev_tx.send(AppEvent::PlayerStateChanged(PlayerState::Loading));
        let quality = match q_level.as_str() {
          "standard" => AudioQualityLevel::Standard,
          "higher" => AudioQualityLevel::Higher,
          "lossless" => AudioQualityLevel::Lossless,
          "hires" => AudioQualityLevel::HiRes,
          _ => AudioQualityLevel::ExHigh,
        };
        match client.song().get_song_url(&track_id, quality).await {
          Ok(Some(url)) => {
            if pt_counter.load(Ordering::SeqCst) != play_token {
              return;
            }
            info!("获取到音频直链: {url}");
            match crate::engine::fetch_http_stream(&url, Some(cancel_flag_task)).await {
              Ok(source) => {
                if pt_counter.load(Ordering::SeqCst) != play_token {
                  source.cancel();
                  return;
                }
                let p_load = p.clone();
                let duration_ms = tokio::task::spawn_blocking(move || {
                  let mut player = p_load.lock();
                  player.load_source(source, true).unwrap_or(0)
                })
                .await
                .unwrap_or(0);
                if pt_counter.load(Ordering::SeqCst) != play_token {
                  return;
                }
                st.write().duration_ms = duration_ms;
                st.write().player_state = PlayerState::Playing;

                if let Some(track) = known_track.clone() {
                  let _ = ev_tx.send(AppEvent::TrackChanged(Some(track.clone())));
                  scrobble.on_track_started(track, None);
                } else if let Some(rx) = detail_rx {
                  let scrobble_wait = scrobble.clone();
                  let pt_wait = pt_counter.clone();
                  let token_wait = play_token;
                  tokio::spawn(async move {
                    if let Ok(track) = rx.await
                      && pt_wait.load(Ordering::SeqCst) == token_wait
                    {
                      scrobble_wait.on_track_started(track, None);
                    }
                  });
                }

                let _ = ev_tx.send(AppEvent::PlayerStateChanged(PlayerState::Playing));
                let _ = ev_tx.send(AppEvent::ProgressUpdated {
                  position_ms: 0,
                  duration_ms,
                });
                PlaylistService::refresh_stats(&db, &ev_tx);
              }
              Err(e) => {
                if pt_counter.load(Ordering::SeqCst) != play_token {
                  return;
                }
                tracing::error!("音频流获取失败: {e}");
                let _ = ev_tx.send(AppEvent::PlayerStateChanged(PlayerState::Error));
              }
            }
          }
          Ok(None) => {
            if pt_counter.load(Ordering::SeqCst) != play_token {
              return;
            }
            tracing::warn!("该曲目无可用播放链接 (可能需要 VIP 或无版权)");
            let _ = ev_tx.send(AppEvent::PlayerStateChanged(PlayerState::Error));
          }
          Err(e) => {
            if pt_counter.load(Ordering::SeqCst) != play_token {
              return;
            }
            tracing::error!("解析播放链接失败: {e}");
            let _ = ev_tx.send(AppEvent::PlayerStateChanged(PlayerState::Error));
          }
        }
      });
    }
  }

  /// 启动音频播放状态与频谱轮询任务
  pub fn spawn_playback_ticker(ctx: &AppContext, rt: &tokio::runtime::Runtime) {
    let player = ctx.player.clone();
    let ev_tx = ctx.event_tx.clone();
    let scrobble = ctx.scrobble_svc.clone();
    let timeline = ctx.lyric_timeline.clone();
    let action_tx = ctx.action_tx.clone();

    rt.spawn(async move {
      let mut interval = tokio::time::interval(Duration::from_millis(80));
      let mut visualizer_active = false;
      let mut auto_next_triggered = false;
      loop {
        interval.tick().await;
        let (pos, dur, is_playing, spectrum_opt) = {
          let p = player.lock();
          let is_playing = p.state() == PlayerState::Playing;
          let spectrum = if is_playing || visualizer_active {
            Some(p.get_spectrum())
          } else {
            None
          };
          (p.position_ms(), p.duration_ms(), is_playing, spectrum)
        };

        if let Some(spectrum) = spectrum_opt {
          if is_playing {
            visualizer_active = true;
            if crate::engine::update_spectrum(spectrum) {
              let _ = ev_tx.send(AppEvent::VisualizerUpdated);
            }
          } else if visualizer_active {
            let is_all_zero = spectrum.iter().all(|&v| v <= 0.01);
            if crate::engine::update_spectrum(spectrum) {
              let _ = ev_tx.send(AppEvent::VisualizerUpdated);
            }
            if is_all_zero {
              visualizer_active = false;
            }
          }
        }

        if is_playing && dur > 0 {
          let _ = ev_tx.send(AppEvent::ProgressUpdated {
            position_ms: pos,
            duration_ms: dur,
          });
          let _ = scrobble.on_progress_tick(pos).await;

          let tl = timeline.read();
          if !tl.is_empty() {
            let active_index = tl.find_active_index(pos);
            let word_progress = active_index
              .map(|idx| tl.line_progress(idx, pos))
              .unwrap_or(0.0);
            let interlude = tl.detect_interlude(pos);
            let _ = ev_tx.send(AppEvent::LyricProgressUpdated {
              active_index: active_index.map(|i| i as i32).unwrap_or(-1),
              word_progress,
              is_interlude: interlude.active,
              interlude_dots: interlude.dots as i32,
            });
          }

          if pos >= dur.saturating_sub(100) {
            if !auto_next_triggered {
              auto_next_triggered = true;
              let _ = action_tx.send(UiAction::NextTrack);
            }
          } else if pos < dur.saturating_sub(300) {
            auto_next_triggered = false;
          }
        } else if !is_playing {
          auto_next_triggered = false;
        }
      }
    });
  }
}
