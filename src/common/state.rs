use std::collections::HashSet;

use super::model::{Track, UserProfile};
use super::types::{NavPage, PlayMode, PlayerState};

/// 内存中的全局应用状态快照
#[derive(Debug, Clone)]
pub struct AppState {
  /// 播放器运行状态
  pub player_state: PlayerState,
  /// 当前播放歌曲
  pub current_track: Option<Track>,
  /// 当前播放进度 (毫秒)
  pub position_ms: u64,
  /// 当前曲目总时长 (毫秒)
  pub duration_ms: u64,
  /// 当前音量 (0.0 ~ 1.0)
  pub volume: f32,
  /// 静音前的音量。`Some` 表示当前处于静音会话。
  pub volume_before_mute: Option<f32>,
  /// 当前循环模式
  pub play_mode: PlayMode,
  /// 当前登录用户信息 (None 为未登录)
  pub current_user: Option<UserProfile>,
  /// 当前活动页面
  pub current_page: NavPage,
  /// 当前播放队列
  pub queue: Vec<Track>,
  /// 当前播放索引
  pub queue_index: usize,
  /// 每日推荐歌曲缓存
  pub daily_songs: Vec<Track>,
  /// 我喜欢的歌曲缓存
  pub liked_songs: Vec<Track>,
  /// 全屏歌词浮层展开状态
  pub is_lyric_expanded: bool,
  /// 播放队列抽屉展开状态
  pub is_queue_open: bool,
  /// 侧边栏折叠状态
  pub is_sidebar_collapsed: bool,
  /// 均衡器面板打开状态
  pub is_equalizer_open: bool,
  /// 设置弹窗打开状态
  pub is_settings_open: bool,
  /// 深色模式开关
  pub is_dark_mode: bool,
  /// 封面自适应动态取色开关
  pub dynamic_color_enabled: bool,
  /// 自定义十六进制颜色
  pub custom_color_hex: String,
  /// 歌手热门歌曲缓存
  pub artist_hot_songs: Vec<Track>,
  /// 页面路由导航历史栈
  pub nav_history: Vec<NavPage>,
  /// 页面路由前进历史栈
  pub forward_history: Vec<NavPage>,
  /// 当前正在查看的歌单 ID
  pub current_playlist_id: Option<String>,
  /// 当前正在查看的歌手 ID
  pub current_artist_id: Option<String>,
}

impl AppState {
  /// 获取当前用户已收藏曲目 ID 集合快照
  pub fn liked_ids_set(&self) -> HashSet<String> {
    self.liked_songs.iter().map(|t| t.id.clone()).collect()
  }
}

impl Default for AppState {
  fn default() -> Self {
    Self {
      player_state: PlayerState::Idle,
      current_track: None,
      position_ms: 0,
      duration_ms: 0,
      volume: 0.8,
      volume_before_mute: None,
      play_mode: PlayMode::Sequence,
      current_user: None,
      current_page: NavPage::Home,
      queue: Vec::new(),
      queue_index: 0,
      daily_songs: Vec::new(),
      liked_songs: Vec::new(),
      artist_hot_songs: Vec::new(),
      is_lyric_expanded: false,
      is_queue_open: false,
      is_sidebar_collapsed: false,
      is_equalizer_open: false,
      is_settings_open: false,
      is_dark_mode: true,
      dynamic_color_enabled: false,
      custom_color_hex: "#fe7971".to_string(),
      nav_history: Vec::new(),
      forward_history: Vec::new(),
      current_playlist_id: None,
      current_artist_id: None,
    }
  }
}
