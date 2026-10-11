pub mod channel;
pub mod dispatcher;
pub mod state;
pub mod sync_lyric;
pub mod sync_nav;
pub mod sync_player;
pub mod sync_theme;
pub mod sync_user;

use parking_lot::RwLock;
use slint::{ComponentHandle, Weak};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::common::types::{NavPage, PlayMode};
use channel::{AppEvent, UiAction};
use state::AppState;
use sync_lyric::SyncLyric;
use sync_nav::SyncNav;
use sync_player::SyncPlayer;
pub use sync_theme::SyncTheme;
use sync_user::SyncUser;

// 引入由 Slint 编译生成的模块
slint::include_modules!();

/// UI 与异步运行时双向通信桥接中心
pub struct AppBridge {
  pub state: Arc<RwLock<AppState>>,
  pub action_tx: mpsc::UnboundedSender<UiAction>,
}

impl AppBridge {
  /// 将 Slint 窗口事件绑定到异步通道
  pub fn attach(
    ui: &MainWindow,
    state: Arc<RwLock<AppState>>,
    action_tx: mpsc::UnboundedSender<UiAction>,
  ) {
    // 1. 绑定播放器控制回调
    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_toggle_play(move || {
      let _ = tx.send(UiAction::TogglePlayPause);
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_prev_track(move || {
      let _ = tx.send(UiAction::PrevTrack);
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_next_track(move || {
      let _ = tx.send(UiAction::NextTrack);
    });

    let tx = action_tx.clone();
    let state_clone = state.clone();
    ui.global::<PlayerActions>().on_seek(move |ratio| {
      let duration = state_clone.read().duration_ms;
      let target_ms = (ratio.clamp(0.0, 1.0) * duration as f32) as u64;
      let _ = tx.send(UiAction::SeekTo(target_ms));
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_change_volume(move |vol| {
      let _ = tx.send(UiAction::SetVolume(vol.clamp(0.0, 1.0)));
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_toggle_mute(move || {
      let _ = tx.send(UiAction::ToggleMute);
    });

    let tx = action_tx.clone();
    let state_clone = state.clone();
    ui.global::<PlayerActions>().on_toggle_play_mode(move || {
      let current_mode = state_clone.read().play_mode;
      let next_mode = match current_mode {
        PlayMode::Sequence => PlayMode::RepeatOne,
        PlayMode::RepeatOne => PlayMode::Shuffle,
        PlayMode::Shuffle => PlayMode::Sequence,
        PlayMode::RepeatAll => PlayMode::Sequence,
      };
      let _ = tx.send(UiAction::SetPlayMode(next_mode));
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_toggle_lyric_view(move || {
      let _ = tx.send(UiAction::ToggleLyricView);
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>()
      .on_toggle_queue_drawer(move || {
        let _ = tx.send(UiAction::ToggleQueueDrawer);
      });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_play_track_by_id(move |id| {
      let _ = tx.send(UiAction::PlayTrack(id.to_string()));
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>()
      .on_play_all_from_list(move |list_id| {
        let _ = tx.send(UiAction::PlayAllFromList(list_id.to_string()));
      });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>()
      .on_toggle_track_like(move |track_id| {
        let _ = tx.send(UiAction::ToggleTrackLike(track_id.to_string()));
      });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_clear_queue(move || {
      let _ = tx.send(UiAction::ClearQueue);
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>()
      .on_remove_queue_track(move |idx| {
        let _ = tx.send(UiAction::RemoveQueueTrack(idx as usize));
      });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_cycle_quality(move || {
      let _ = tx.send(UiAction::CycleQuality);
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_select_quality(move |q| {
      let _ = tx.send(UiAction::SelectQuality(q.to_string()));
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_open_comments(move || {
      let _ = tx.send(UiAction::OpenComments);
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_close_comments(move || {
      let _ = tx.send(UiAction::CloseComments);
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>().on_play_random(move || {
      let _ = tx.send(UiAction::PlayRandomTrack);
    });

    let tx = action_tx.clone();
    ui.global::<PlayerActions>()
      .on_collect_playlist(move |pl_id| {
        let _ = tx.send(UiAction::CollectPlaylist(pl_id.to_string()));
      });

    // 2. 绑定导航回调
    let tx = action_tx.clone();
    ui.global::<NavActions>().on_navigate_to(move |page_str| {
      let page = match page_str.as_str() {
        "home" => NavPage::Home,
        "daily" => NavPage::Daily,
        "liked" => NavPage::Liked,
        "playlist" => NavPage::Playlist,
        "library" => NavPage::Library,
        "search" => NavPage::Search,
        "artist" => NavPage::Artist,
        "stats" => NavPage::Stats,
        _ => NavPage::Home,
      };
      let _ = tx.send(UiAction::NavigateTo(page));
    });

    let tx = action_tx.clone();
    ui.global::<NavActions>().on_go_back(move || {
      let _ = tx.send(UiAction::GoBack);
    });

    let tx = action_tx.clone();
    ui.global::<NavActions>().on_go_forward(move || {
      let _ = tx.send(UiAction::GoForward);
    });

    let tx = action_tx.clone();
    ui.global::<NavActions>().on_trigger_search(move |kw| {
      let _ = tx.send(UiAction::Search(kw.to_string()));
    });

    let tx = action_tx.clone();
    ui.global::<NavActions>()
      .on_open_playlist_detail(move |id| {
        let _ = tx.send(UiAction::OpenPlaylistDetail(id.to_string()));
      });

    let tx = action_tx.clone();
    ui.global::<NavActions>().on_toggle_sidebar(move || {
      let _ = tx.send(UiAction::ToggleSidebar);
    });

    let tx = action_tx.clone();
    ui.global::<NavActions>()
      .on_navigate_to_artist(move |id, name| {
        let _ = tx.send(UiAction::NavigateToArtist {
          id: id.to_string(),
          name: name.to_string(),
        });
      });

    // 3. 绑定用户与认证动作
    let tx = action_tx.clone();
    ui.global::<AuthActions>().on_open_login(move || {
      let _ = tx.send(UiAction::OpenLoginModal);
    });

    let tx = action_tx.clone();
    ui.global::<AuthActions>().on_close_login(move || {
      let _ = tx.send(UiAction::CloseLoginModal);
    });

    let tx = action_tx.clone();
    ui.global::<AuthActions>().on_refresh_qr(move || {
      let _ = tx.send(UiAction::RefreshQr);
    });

    let tx = action_tx.clone();
    ui.global::<AuthActions>().on_logout(move || {
      let _ = tx.send(UiAction::Logout);
    });

    // 4. 绑定歌词交互动作
    let tx = action_tx.clone();
    ui.global::<LyricActions>().on_seek_to_timestamp(move |ms| {
      let _ = tx.send(UiAction::SeekTo(ms as u64));
    });

    let tx = action_tx.clone();
    ui.global::<LyricActions>().on_copy_current_lyric(move || {
      let _ = tx.send(UiAction::CopyCurrentLyric);
    });

    let tx = action_tx.clone();
    ui.global::<LyricActions>().on_adjust_offset(move |sec| {
      let _ = tx.send(UiAction::AdjustLyricOffset(sec));
    });

    // 5. 绑定主题与均衡器动作
    let tx = action_tx.clone();
    ui.global::<ThemeActions>().on_set_custom_color(move |hex| {
      let _ = tx.send(UiAction::SetCustomColor(hex.to_string()));
    });

    let tx = action_tx.clone();
    ui.global::<ThemeActions>()
      .on_toggle_dynamic_color(move || {
        let _ = tx.send(UiAction::ToggleDynamicColor);
      });

    let tx = action_tx.clone();
    ui.global::<ThemeActions>().on_toggle_dark_mode(move || {
      let _ = tx.send(UiAction::ToggleDarkMode);
    });

    let tx = action_tx.clone();
    ui.global::<ThemeActions>().on_open_equalizer(move || {
      let _ = tx.send(UiAction::OpenEqualizer);
    });

    let tx = action_tx.clone();
    ui.global::<ThemeActions>().on_close_equalizer(move || {
      let _ = tx.send(UiAction::CloseEqualizer);
    });

    let tx = action_tx.clone();
    ui.global::<ThemeActions>().on_open_settings(move || {
      let _ = tx.send(UiAction::OpenSettings);
    });

    let tx = action_tx.clone();
    ui.global::<ThemeActions>().on_close_settings(move || {
      let _ = tx.send(UiAction::CloseSettings);
    });

    let tx = action_tx.clone();
    ui.global::<ThemeActions>()
      .on_set_eq_gain(move |band, gain| {
        let _ = tx.send(UiAction::SetEqGain(band as usize, gain));
      });

    let tx = action_tx.clone();
    ui.global::<ThemeActions>().on_set_eq_preset(move |preset| {
      let _ = tx.send(UiAction::SetEqPreset(preset.to_string()));
    });

    // 6. 绑定本地音乐库与下载管理
    let tx = action_tx.clone();
    ui.global::<LibraryActions>().on_scan_local_folder(move || {
      let _ = tx.send(UiAction::ScanLocalFolder);
    });

    let tx = action_tx.clone();
    ui.global::<LibraryActions>().on_download_track(move |id| {
      let _ = tx.send(UiAction::DownloadTrack(id.to_string()));
    });

    let tx = action_tx.clone();
    ui.global::<LibraryActions>()
      .on_switch_library_tab(move |tab| {
        let _ = tx.send(UiAction::SwitchLibraryTab(tab.to_string()));
      });

    let tx = action_tx.clone();
    ui.global::<LibraryActions>()
      .on_open_download_folder(move || {
        let _ = tx.send(UiAction::OpenDownloadFolder);
      });

    let tx = action_tx.clone();
    ui.global::<LibraryActions>()
      .on_create_new_playlist(move || {
        let _ = tx.send(UiAction::CreateNewPlaylist);
      });

    // 7. 绑定窗口控制动作
    let weak_min = ui.as_weak();
    ui.global::<WindowActions>().on_minimize(move || {
      if let Some(ui) = weak_min.upgrade() {
        ui.window().set_minimized(true);
      }
    });

    let weak_max = ui.as_weak();
    ui.global::<WindowActions>().on_toggle_maximize(move || {
      if let Some(ui) = weak_max.upgrade() {
        let is_max = ui.window().is_maximized();
        ui.window().set_maximized(!is_max);
      }
    });

    let weak_close = ui.as_weak();
    ui.global::<WindowActions>().on_close(move || {
      if let Some(ui) = weak_close.upgrade() {
        let _ = ui.window().hide();
      }
      let _ = slint::quit_event_loop();
    });
  }

  /// 将后台分发的 AppEvent 推送到 Slint UI 线程执行更新
  pub fn dispatch_event(
    weak_ui: &Weak<MainWindow>,
    state: &Arc<RwLock<AppState>>,
    event: AppEvent,
  ) {
    let state = state.clone();
    let _ = weak_ui.upgrade_in_event_loop(move |ui| match event {
      AppEvent::PlayerStateChanged(state) => {
        SyncPlayer::sync_player_state(&ui, state);
      }
      AppEvent::ProgressUpdated {
        position_ms,
        duration_ms,
      } => {
        SyncPlayer::sync_progress(&ui, position_ms, duration_ms);
      }
      AppEvent::TrackChanged(track_opt) => {
        SyncPlayer::sync_track(&ui, track_opt.as_ref());
      }
      AppEvent::VolumeChanged(vol) => {
        SyncPlayer::sync_volume(&ui, vol);
      }
      AppEvent::PlayModeChanged(mode) => {
        SyncPlayer::sync_play_mode(&ui, mode);
      }
      AppEvent::QueueUpdated(queue) => {
        let liked_ids = state.read().liked_ids_set();
        SyncPlayer::sync_queue(&ui, &queue, &liked_ids);
      }
      AppEvent::VisualizerUpdated => {
        SyncPlayer::sync_visualizer(&ui);
      }
      AppEvent::QualityChanged(q) => {
        SyncPlayer::sync_quality(&ui, &q);
      }
      AppEvent::LyricsLoaded(lines) => {
        SyncLyric::sync_lyrics(&ui, &lines);
      }
      AppEvent::LyricProgressUpdated {
        active_index,
        word_progress,
        is_interlude,
        interlude_dots,
      } => {
        SyncLyric::sync_progress(
          &ui,
          active_index,
          word_progress,
          is_interlude,
          interlude_dots,
        );
      }
      AppEvent::UserChanged(user_opt) => {
        SyncUser::sync_user(&ui, user_opt.as_ref());
      }
      AppEvent::QrCodeGenerated { svg_data, .. } => {
        SyncUser::sync_qr_image(&ui, &svg_data);
      }
      AppEvent::QrStatusChanged(status_text) => {
        SyncUser::sync_qr_status(&ui, &status_text);
      }
      AppEvent::NavPageChanged(page) => {
        SyncNav::sync_page(&ui, page);
      }
      AppEvent::CanGoBackChanged(can) => {
        SyncNav::sync_can_go_back(&ui, can);
      }
      AppEvent::CanGoForwardChanged(can) => {
        SyncNav::sync_can_go_forward(&ui, can);
      }
      AppEvent::TrackLikeStateChanged(liked) => {
        SyncPlayer::sync_track_liked(&ui, liked);
      }
      AppEvent::TrackLikeToggled { track_id, is_liked } => {
        SyncNav::sync_track_like_toggled(&ui, &track_id, is_liked);
        SyncPlayer::sync_track_like_toggled(&ui, &track_id, is_liked);
      }
      AppEvent::PlaylistsLoaded(playlists) => {
        SyncNav::sync_playlists(&ui, &playlists);
      }
      AppEvent::DailySongsLoaded(tracks) => {
        let liked_ids = state.read().liked_ids_set();
        SyncNav::sync_daily_songs(&ui, &tracks, &liked_ids);
      }
      AppEvent::LikedSongsLoaded(tracks) => {
        SyncNav::sync_liked_songs(&ui, &tracks);
      }
      AppEvent::LocalTracksLoaded(tracks) => {
        let liked_ids = state.read().liked_ids_set();
        SyncNav::sync_local_tracks(&ui, &tracks, &liked_ids);
      }
      AppEvent::SearchResultsLoaded { songs, playlists } => {
        let liked_ids = state.read().liked_ids_set();
        SyncNav::sync_search_results(&ui, &songs, &playlists, &liked_ids);
      }
      AppEvent::PlaylistDetailLoaded { playlist, tracks } => {
        let liked_ids = state.read().liked_ids_set();
        SyncNav::sync_playlist_detail(&ui, &playlist, &tracks, &liked_ids);
      }
      AppEvent::HomeRecommendSongsLoaded(songs) => {
        let liked_ids = state.read().liked_ids_set();
        SyncNav::sync_home_recommend_songs(&ui, &songs, &liked_ids);
      }
      AppEvent::ShowToast(msg) => {
        tracing::info!("Toast: {}", msg);
        let toast = ui.global::<ToastState>();
        toast.set_text(msg.into());
        toast.set_is_open(true);
        let ui_weak = ui.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(2500), move || {
          if let Some(ui) = ui_weak.upgrade() {
            ui.global::<ToastState>().set_is_open(false);
          }
        });
      }
      AppEvent::CoverLoaded { path, accent_color } => {
        if let Ok(image) = slint::Image::load_from_path(&path) {
          crate::service::CoverService::insert_cached_image(&path.to_string_lossy(), image.clone());
          SyncPlayer::sync_cover(&ui, image, accent_color);
        }
      }
      AppEvent::PlaylistCoverLoaded(path) => {
        if let Ok(image) = slint::Image::load_from_path(&path) {
          crate::service::CoverService::insert_cached_image(&path.to_string_lossy(), image.clone());
          SyncNav::sync_playlist_cover(&ui, image);
        }
      }
      AppEvent::ThemeColorChanged(color) => {
        let is_dark = ui.global::<crate::bridge::Theme>().get_is_dark();
        SyncTheme::sync_theme(&ui, color, is_dark);
      }
      AppEvent::CustomColorChanged(hex) => {
        ui.global::<crate::bridge::ThemeState>()
          .set_custom_color_hex(hex.into());
      }
      AppEvent::DynamicColorChanged(enabled) => {
        ui.global::<crate::bridge::ThemeState>()
          .set_dynamic_color_enabled(enabled);
      }
      AppEvent::DarkModeChanged(is_dark) => {
        ui.global::<crate::bridge::Theme>().set_is_dark(is_dark);
        let raw = ui.global::<crate::bridge::Theme>().get_raw_primary();
        SyncTheme::sync_theme(&ui, raw, is_dark);
      }
      AppEvent::LyricViewToggled(val) => {
        ui.global::<PlayerState>().set_is_lyric_expanded(val);
      }
      AppEvent::QueueDrawerToggled(val) => {
        ui.global::<PlayerState>().set_is_queue_open(val);
      }
      AppEvent::SidebarToggled(val) => {
        ui.global::<NavState>().set_sidebar_collapsed(val);
      }
      AppEvent::EqualizerToggled(val) => {
        ui.global::<crate::bridge::ThemeState>()
          .set_is_equalizer_open(val);
      }
      AppEvent::SettingsToggled(val) => {
        ui.global::<crate::bridge::ThemeState>()
          .set_is_settings_open(val);
      }
      AppEvent::LoginModalToggled(val) => {
        SyncUser::sync_login_modal(&ui, val);
      }
      AppEvent::HotKeywordsLoaded(kw) => {
        SyncNav::sync_hot_keywords(&ui, &kw);
      }
      AppEvent::SystemDateLoaded {
        day,
        month,
        weekday,
      } => {
        SyncNav::sync_system_date(&ui, &day, &month, &weekday);
      }
      AppEvent::CommentsModalToggled(val) => {
        SyncNav::sync_comments_modal(&ui, val);
      }
      AppEvent::CommentsLoaded(comments) => {
        SyncNav::sync_comments(&ui, &comments);
      }
      AppEvent::ArtistLoaded {
        artist,
        desc,
        tracks,
      } => {
        let liked_ids = state.read().liked_ids_set();
        SyncNav::sync_artist_detail(&ui, &artist, &desc, &tracks, &liked_ids);
      }
      AppEvent::DownloadsLoaded(tasks) => {
        SyncNav::sync_downloads(&ui, &tasks);
      }
      AppEvent::LibraryTabChanged(tab) => {
        SyncNav::sync_library_tab(&ui, &tab);
      }
      AppEvent::RecentPlaysLoaded(tracks) => {
        let liked_ids = state.read().liked_ids_set();
        SyncNav::sync_continue_listening(&ui, &tracks, &liked_ids);
      }
      AppEvent::ListeningStatsLoaded { greeting, stats } => {
        SyncNav::sync_listening_stats(&ui, &greeting, &stats);
      }
    });
  }
}
