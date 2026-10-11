use slint::{ComponentHandle, Model, VecModel};
use std::rc::Rc;

use crate::bridge::MainWindow;
use crate::bridge::{PageDataState, PlayerState, TrackUi};
use crate::common::model::Track;
use crate::common::types::{PlayMode, PlayerState as CorePlayerState};

/// 格式化毫秒时长为 mm:ss
pub fn format_time_ms(ms: u64) -> String {
  let total_secs = ms / 1000;
  let mins = total_secs / 60;
  let secs = total_secs % 60;
  format!("{:02}:{:02}", mins, secs)
}

/// 播放器核心状态属性同步器
pub struct SyncPlayer;

impl SyncPlayer {
  /// 同步播放状态 (Playing, Paused, Loading, Stopped, Error)
  pub fn sync_player_state(ui: &MainWindow, state: CorePlayerState) {
    let global = ui.global::<PlayerState>();
    match state {
      CorePlayerState::Playing => {
        global.set_is_playing(true);
        global.set_is_loading(false);
      }
      CorePlayerState::Paused => {
        global.set_is_playing(false);
        global.set_is_loading(false);
      }
      CorePlayerState::Loading => {
        global.set_is_loading(true);
      }
      CorePlayerState::Stopped | CorePlayerState::Idle => {
        global.set_is_playing(false);
        global.set_is_loading(false);
      }
      CorePlayerState::Error => {
        global.set_is_playing(false);
        global.set_is_loading(false);
      }
    }
  }

  /// 同步当前播放时间进度
  pub fn sync_progress(ui: &MainWindow, position_ms: u64, duration_ms: u64) {
    let global = ui.global::<PlayerState>();
    global.set_position_ms(position_ms as i32);
    global.set_duration_ms(duration_ms as i32);
    global.set_position_text(format_time_ms(position_ms).into());
    global.set_duration_text(format_time_ms(duration_ms).into());

    let ratio = if duration_ms > 0 {
      (position_ms as f32 / duration_ms as f32).clamp(0.0, 1.0)
    } else {
      0.0
    };
    global.set_progress_ratio(ratio);
  }

  /// 同步当前正在播放的曲目元数据
  pub fn sync_track(ui: &MainWindow, track_opt: Option<&Track>) {
    let global = ui.global::<PlayerState>();
    if let Some(track) = track_opt {
      global.set_track_id(track.id.clone().into());
      global.set_track_title(track.title.clone().into());
      global.set_track_artists(track.artists_text().into());
      global.set_track_album(
        track
          .album
          .as_ref()
          .map(|a| a.name.clone())
          .unwrap_or_default()
          .into(),
      );
      let cover_url = track.cover_url.clone().unwrap_or_default();
      global.set_track_cover(cover_url.clone().into());
      global.set_duration_ms(track.duration_ms as i32);
      global.set_duration_text(format_time_ms(track.duration_ms).into());

      let (img, has_cover) = crate::service::CoverService::try_get_cached_image(&cover_url);
      if has_cover {
        global.set_track_cover_image(img);
        global.set_has_track_cover(true);
      } else {
        global.set_has_track_cover(false);
      }
    } else {
      global.set_track_id("".into());
      global.set_track_title("等待播放".into());
      global.set_track_artists("网易云音乐".into());
      global.set_track_album("".into());
      global.set_track_cover("".into());
      global.set_has_track_cover(false);
    }
  }

  /// 同步当前曲目收藏红心状态
  pub fn sync_track_liked(ui: &MainWindow, is_liked: bool) {
    ui.global::<PlayerState>().set_track_is_liked(is_liked);
  }

  /// 同步播放音量与静音状态
  pub fn sync_volume(ui: &MainWindow, volume: f32) {
    let global = ui.global::<PlayerState>();
    global.set_volume(volume);
    global.set_is_muted(volume <= 0.001);
  }

  /// 同步循环模式
  pub fn sync_play_mode(ui: &MainWindow, mode: PlayMode) {
    let mode_str = match mode {
      PlayMode::Sequence => "sequence",
      PlayMode::RepeatOne => "single",
      PlayMode::Shuffle => "shuffle",
      PlayMode::RepeatAll => "loop",
    };
    ui.global::<PlayerState>().set_play_mode(mode_str.into());
  }

  /// 同步播放队列曲目
  pub fn sync_queue(
    ui: &MainWindow,
    queue: &[Track],
    liked_ids: &std::collections::HashSet<String>,
  ) {
    let global = ui.global::<PlayerState>();
    let ui_tracks: Vec<TrackUi> = queue
      .iter()
      .map(|t| {
        let cover_url = t.cover_url.clone().unwrap_or_default();
        let (cover_image, has_cover) =
          crate::service::CoverService::try_get_cached_image(&cover_url);
        TrackUi {
          id: t.id.clone().into(),
          artist_id: t
            .artists
            .first()
            .map(|a| a.id.clone())
            .unwrap_or_default()
            .into(),
          title: t.title.clone().into(),
          artists: t.artists_text().into(),
          album: t
            .album
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_default()
            .into(),
          duration_str: format_time_ms(t.duration_ms).into(),
          duration_ms: t.duration_ms as i32,
          cover_url: cover_url.into(),
          cover_image,
          has_cover,
          is_vip: t.fee == 1,
          is_liked: liked_ids.contains(&t.id),
        }
      })
      .collect();
    global.set_queue_count(ui_tracks.len() as i32);
    let model = Rc::new(VecModel::from(ui_tracks));
    global.set_queue_tracks(model.into());
  }

  /// 更新播放队列和当前曲目中的收藏红心状态
  pub fn sync_track_like_toggled(ui: &MainWindow, track_id: &str, is_liked: bool) {
    let global = ui.global::<PlayerState>();
    if global.get_track_id().as_str() == track_id {
      global.set_track_is_liked(is_liked);
    }
    let queue_model = global.get_queue_tracks();
    let count = queue_model.row_count();
    if count == 0 {
      return;
    }
    let mut changed = false;
    let mut items: Vec<TrackUi> = (0..count).filter_map(|i| queue_model.row_data(i)).collect();
    for item in &mut items {
      if item.id.as_str() == track_id {
        item.is_liked = is_liked;
        changed = true;
      }
    }
    if changed {
      global.set_queue_tracks(Rc::new(VecModel::from(items)).into());
    }
  }

  /// 更新最新瞬态频谱数据
  pub fn update_spectrum(spectrum: [f32; 32]) -> bool {
    crate::engine::update_spectrum(spectrum)
  }

  /// 同步音频频谱数据
  pub fn sync_visualizer(ui: &MainWindow) {
    let bars = crate::engine::take_spectrum();
    let model = Rc::new(VecModel::from(bars.to_vec()));
    ui.global::<PageDataState>()
      .set_visualizer_bars(model.into());
  }

  /// 重置频谱状态与在飞标志位
  pub fn reset_visualizer() {
    crate::engine::reset_spectrum();
  }

  /// 同步音质角标
  pub fn sync_quality(ui: &MainWindow, badge: &str) {
    ui.global::<PlayerState>().set_quality_badge(badge.into());
  }

  /// 同步歌曲封面图像与动态取色
  pub fn sync_cover(ui: &MainWindow, img: slint::Image, accent_color: slint::Color) {
    let global = ui.global::<PlayerState>();
    global.set_track_cover_image(img);
    global.set_has_track_cover(true);

    if ui
      .global::<crate::bridge::ThemeState>()
      .get_dynamic_color_enabled()
    {
      let is_dark = ui.global::<crate::bridge::Theme>().get_is_dark();
      crate::bridge::SyncTheme::sync_theme(ui, accent_color, is_dark);
    }
  }
}
