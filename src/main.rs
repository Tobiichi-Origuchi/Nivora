use chrono::Datelike;
use parking_lot::{Mutex, RwLock};
use slint::ComponentHandle;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::mpsc;
use tracing::info;

use nivora_lib::api::NcmClient;
use nivora_lib::bridge::channel::{AppEvent, UiAction};
use nivora_lib::bridge::dispatcher::ActionDispatcher;
use nivora_lib::bridge::state::AppState;
use nivora_lib::bridge::{AppBridge, MainWindow};
use nivora_lib::common::model::Track;
use nivora_lib::common::types::AudioQualityLevel;
use nivora_lib::data::repo_history::HistoryRepo;
use nivora_lib::data::{Database, SettingsRepo};
use nivora_lib::engine::InnerPlayer;
use nivora_lib::lyric::LyricTimeline;
use nivora_lib::service::{
  CoverService, LibraryService, PlaybackQueue, PlaylistService, ScrobbleService,
};

fn main() {
  // 1. 初始化日志
  tracing_subscriber::fmt()
    .with_env_filter(
      tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()),
    )
    .init();

  // 2. 初始化数据库
  let db = Database::open_default().unwrap_or_else(|e| {
    tracing::warn!("无法打开系统数据目录数据库，将使用内存数据库: {e}");
    Database::open_in_memory().expect("无法初始化内存数据库")
  });

  // 3. 从数据库加载配置
  let config = Arc::new(Mutex::new(SettingsRepo::load_config(&db)));
  let (
    initial_vol,
    initial_dark,
    initial_quality,
    initial_theme_color,
    initial_custom_hex,
    initial_dynamic_color,
    initial_play_mode,
    initial_sidebar,
    initial_eq_preset,
    initial_eq_gains,
  ) = {
    let cfg = config.lock();
    (
      cfg.volume,
      cfg.dark_mode,
      cfg.audio_quality,
      cfg.theme_color.clone(),
      cfg.custom_color_hex.clone(),
      cfg.dynamic_color_enabled,
      cfg.play_mode,
      cfg.sidebar_collapsed,
      cfg.eq_preset.clone(),
      cfg.eq_gains.clone(),
    )
  };

  // 4. 构建 Tokio 运行时
  let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("无法初始化 Tokio 运行时");

  // 5. 初始化音频引擎与全局状态容器
  let player = Arc::new(Mutex::new(InnerPlayer::new()));
  player.lock().set_volume(initial_vol);
  if !initial_eq_preset.is_empty() {
    player.lock().set_eq_preset(&initial_eq_preset);
  }
  for (band, gain) in initial_eq_gains.iter().enumerate() {
    player.lock().set_eq_gain(band, *gain);
  }

  // 从数据库恢复上次播放队列 (如有)
  let saved_queue: Vec<Track> =
    nivora_lib::data::SettingsRepo::get_json(&db, "playback_queue").unwrap_or_default();

  let playback_queue = Arc::new(Mutex::new(PlaybackQueue::with_mode(initial_play_mode)));
  if !saved_queue.is_empty() {
    playback_queue.lock().set_queue(saved_queue.clone());
  }

  let state = Arc::new(RwLock::new(AppState {
    volume: initial_vol,
    is_dark_mode: initial_dark,
    dynamic_color_enabled: initial_dynamic_color,
    custom_color_hex: initial_custom_hex.clone(),
    play_mode: initial_play_mode,
    is_sidebar_collapsed: initial_sidebar,
    queue: saved_queue.clone(),
    ..AppState::default()
  }));
  let (action_tx, action_rx) = mpsc::unbounded_channel::<UiAction>();
  let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AppEvent>();

  // 6. 初始化业务服务与数据加载
  let ncm_client = NcmClient::with_db(db.clone());
  let scrobble_svc = ScrobbleService::new(ncm_client.clone(), db.clone());
  let lyric_timeline = Arc::new(RwLock::new(LyricTimeline::new(Vec::new())));
  let (initial_q_str, initial_q_badge) = match initial_quality {
    AudioQualityLevel::Standard => ("standard", "标准"),
    AudioQualityLevel::Higher => ("higher", "较高"),
    AudioQualityLevel::ExHigh => ("exhigh", "极高"),
    AudioQualityLevel::Lossless => ("lossless", "SQ"),
    AudioQualityLevel::HiRes => ("hires", "Hi-Res"),
  };
  let quality_level = Arc::new(Mutex::new(initial_q_str.to_string()));
  let qr_polling_cancel = Arc::new(AtomicBool::new(false));
  let current_extracted_color = Arc::new(Mutex::new(None::<[u8; 4]>));

  // 6. 初始数据预加载
  let init_client = ncm_client.clone();
  let init_db = db.clone();
  let init_ev_tx = event_tx.clone();
  let init_state = state.clone();
  rt.spawn(async move {
    let now = chrono::Local::now();
    let day = format!("{:02}", now.day());
    let month = format!("{}月", now.month());
    let weekday = match now.weekday() {
      chrono::Weekday::Mon => "星期一",
      chrono::Weekday::Tue => "星期二",
      chrono::Weekday::Wed => "星期三",
      chrono::Weekday::Thu => "星期四",
      chrono::Weekday::Fri => "星期五",
      chrono::Weekday::Sat => "星期六",
      chrono::Weekday::Sun => "星期日",
    };
    let _ = init_ev_tx.send(AppEvent::SystemDateLoaded {
      day,
      month,
      weekday: weekday.to_string(),
    });

    // 读取已保存登录状态
    if let Ok(Some(user)) = init_client.auth().get_login_status().await {
      info!("已恢复登录会话: {}", user.nickname);
      let _ = init_ev_tx.send(AppEvent::UserChanged(Some(user)));
    }

    // 读取用户听歌统计与问候语
    if let Ok(stats) = HistoryRepo::get_listening_stats(&init_db) {
      let greeting = PlaylistService::get_time_greeting().to_string();
      let _ = init_ev_tx.send(AppEvent::ListeningStatsLoaded { greeting, stats });
    }

    // 读取热门搜索词
    if let Ok(hot_words) = init_client.search().search_hot_detail().await {
      let _ = init_ev_tx.send(AppEvent::HotKeywordsLoaded(hot_words));
    }

    // 读取本地音乐
    if let Ok(local_tracks) = LibraryService::get_all_local_tracks(&init_db) {
      let _ = init_ev_tx.send(AppEvent::LocalTracksLoaded(local_tracks));
    }

    // 尝试拉取推荐歌单
    if let Ok(playlists) = init_client.playlist().get_personalized_playlists(12).await {
      let _ = init_ev_tx.send(AppEvent::PlaylistsLoaded(playlists.clone()));
      CoverService::spawn_playlist_covers_loader(playlists, init_ev_tx.clone());
    }

    // 尝试拉取首页精选推荐新歌
    if let Ok(newsongs) = init_client.playlist().get_recommend_newsongs(10).await {
      let _ = init_ev_tx.send(AppEvent::HomeRecommendSongsLoaded(newsongs.clone()));
      CoverService::spawn_tracks_covers_loader(
        newsongs,
        10,
        init_ev_tx.clone(),
        AppEvent::HomeRecommendSongsLoaded,
      );
    }

    // 尝试拉取每日推荐歌曲
    if let Ok(daily_tracks) = init_client.playlist().get_recommend_songs().await {
      init_state.write().daily_songs = daily_tracks.clone();
      let _ = init_ev_tx.send(AppEvent::DailySongsLoaded(daily_tracks.clone()));
      let count = daily_tracks.len();
      CoverService::spawn_tracks_covers_loader(
        daily_tracks,
        count,
        init_ev_tx.clone(),
        AppEvent::DailySongsLoaded,
      );
    }

    // 尝试拉取收藏的歌曲
    if let Ok(favs) = HistoryRepo::list_favorites(&init_db, 1000) {
      let liked: Vec<Track> = favs.into_iter().map(|f| f.track).collect();
      init_state.write().liked_songs = liked.clone();
      let _ = init_ev_tx.send(AppEvent::LikedSongsLoaded(liked.clone()));
      let count = liked.len();
      CoverService::spawn_tracks_covers_loader(
        liked,
        count,
        init_ev_tx.clone(),
        AppEvent::LikedSongsLoaded,
      );
    }

    // 读取最近播放足迹
    if let Ok(recent) = HistoryRepo::list_recent_plays(&init_db, 10) {
      let tracks: Vec<Track> = recent
        .into_iter()
        .map(|f| f.track)
        .filter(|t| !t.id.is_empty())
        .collect();
      if !tracks.is_empty() {
        let _ = init_ev_tx.send(AppEvent::RecentPlaysLoaded(tracks.clone()));
        let count = tracks.len();
        CoverService::spawn_tracks_covers_loader(
          tracks,
          count,
          init_ev_tx.clone(),
          AppEvent::RecentPlaysLoaded,
        );
      }
    }

    // 恢复持久化播放队列到 UI
    if !init_state.read().queue.is_empty() {
      let q = init_state.read().queue.clone();
      let _ = init_ev_tx.send(AppEvent::QueueUpdated(q));
    }
  });

  // 7. 启动 UI 动作调度器与音频状态轮询任务
  let app_ctx = nivora_lib::service::AppContext {
    player: player.clone(),
    state: state.clone(),
    event_tx: event_tx.clone(),
    action_tx: action_tx.clone(),
    client: ncm_client.clone(),
    scrobble_svc: scrobble_svc.clone(),
    lyric_timeline: lyric_timeline.clone(),
    queue: playback_queue.clone(),
    playback_svc: playback_queue.clone(),
    quality_level: quality_level.clone(),
    db: db.clone(),
    config: config.clone(),
    current_extracted_color: current_extracted_color.clone(),
    play_token_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    active_cancel_token: Arc::new(Mutex::new(None)),
  };
  let dispatcher = ActionDispatcher::new(app_ctx, qr_polling_cancel.clone());
  dispatcher.clone().spawn(&rt, action_rx);
  dispatcher.spawn_playback_ticker(&rt);

  // 8. 初始化 Slint UI 窗口并桥接
  let ui = MainWindow::new().expect("无法创建 Slint 主窗口");
  AppBridge::attach(&ui, state.clone(), action_tx);

  // 9. 同步持久化状态
  {
    // 9.1 主题色与暗色模式
    let theme_global = ui.global::<nivora_lib::bridge::Theme>();
    theme_global.set_is_dark(initial_dark);
    if let Some(color) = nivora_lib::service::ThemeService::hex_to_color(&initial_theme_color) {
      nivora_lib::bridge::SyncTheme::sync_theme(&ui, color, initial_dark);
    }

    let theme_state = ui.global::<nivora_lib::bridge::ThemeState>();
    theme_state.set_custom_color_hex(initial_custom_hex.into());
    theme_state.set_dynamic_color_enabled(initial_dynamic_color);

    // 9.2 播放器初始状态 (音量、音质 Badge、播放模式)
    let player_state = ui.global::<nivora_lib::bridge::PlayerState>();
    player_state.set_volume(initial_vol);
    player_state.set_quality_badge(initial_q_badge.into());
    let mode_str = match initial_play_mode {
      nivora_lib::common::types::PlayMode::Sequence => "sequence",
      nivora_lib::common::types::PlayMode::RepeatOne => "single",
      nivora_lib::common::types::PlayMode::Shuffle => "shuffle",
      nivora_lib::common::types::PlayMode::RepeatAll => "loop",
    };
    player_state.set_play_mode(mode_str.into());

    // 9.3 侧边栏折叠状态
    let nav_state = ui.global::<nivora_lib::bridge::NavState>();
    nav_state.set_sidebar_collapsed(initial_sidebar);

    // 9.4 均衡器预设显示
    if !initial_eq_preset.is_empty() {
      let page_data = ui.global::<nivora_lib::bridge::PageDataState>();
      page_data.set_eq_preset_name(initial_eq_preset.into());
    }
  }

  // 10. 启动事件转发工作线程
  let weak_ui = ui.as_weak();
  let state_ev = state.clone();
  std::thread::spawn(move || {
    while let Some(event) = event_rx.blocking_recv() {
      AppBridge::dispatch_event(&weak_ui, &state_ev, event);
    }
  });

  // 11. 启动应用
  ui.run().expect("Slint UI 异常退出");

  // 12. 关闭应用

  // 停止音频播放并释放硬件设备
  player.lock().stop();

  // 持久化当前播放状态
  let current_vol = ui.global::<nivora_lib::bridge::PlayerState>().get_volume();
  let _ = nivora_lib::data::SettingsRepo::set(&db, "playback_volume", &current_vol.to_string());
  let current_queue = state.read().queue.clone();
  let _ = nivora_lib::data::SettingsRepo::set_json(&db, "playback_queue", &current_queue);

  if let Err(e) = db.checkpoint() {
    tracing::warn!("SQLite WAL Checkpoint 执行异常: {e}");
  } else {
    info!("SQLite WAL Checkpoint 完成");
  }
}
