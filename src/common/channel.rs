use std::path::PathBuf;

use super::model::{
  Artist, Comment, DownloadTask, Playlist, Track, UserListeningStats, UserProfile,
};
use super::types::{NavPage, PlayMode, PlayerState};
use crate::lyric::LyricLine;

/// 从 UI 界面发出的用户交互动作
#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
  /// 播放指定曲目 (曲目 ID)
  PlayTrack(String),
  /// 播放/暂停切换
  TogglePlayPause,
  /// 上一曲
  PrevTrack,
  /// 下一曲
  NextTrack,
  /// 拖动进度条跳转 (毫秒)
  SeekTo(u64),
  /// 调节音量 (0.0 ~ 1.0)
  SetVolume(f32),
  /// 静音切换
  ToggleMute,
  /// 切换循环播放模式
  SetPlayMode(PlayMode),
  /// 页面导航切换
  NavigateTo(NavPage),
  /// 打开歌单详情
  OpenPlaylistDetail(String),
  /// 播放指定列表中的全部歌曲 (歌单 ID / "daily" / "liked" / "local")
  PlayAllFromList(String),
  /// 切换歌曲收藏红心
  ToggleTrackLike(String),
  /// 提交搜索关键词
  Search(String),
  /// 打开登录弹窗
  OpenLoginModal,
  /// 关闭登录弹窗
  CloseLoginModal,
  /// 刷新二维码
  RefreshQr,
  /// 用户主动注销登录
  Logout,
  /// 展开/收起全屏歌词浮层
  ToggleLyricView,
  /// 展开/收起播放队列侧边抽屉
  ToggleQueueDrawer,
  /// 清空播放队列
  ClearQueue,
  /// 移除队列中指定索引的曲目
  RemoveQueueTrack(usize),
  /// 切换在线音质等级
  CycleQuality,
  /// 直接设置在线音质等级
  SelectQuality(String),
  /// 折叠/展开侧边导航栏
  ToggleSidebar,
  /// 设置自定义十六进制颜色
  SetCustomColor(String),
  /// 切换封面自适应动态取色
  ToggleDynamicColor,
  /// 切换明暗外观模式
  ToggleDarkMode,
  /// 打开均衡器面板
  OpenEqualizer,
  /// 关闭均衡器面板
  CloseEqualizer,
  /// 打开设置弹窗
  OpenSettings,
  /// 关闭设置弹窗
  CloseSettings,
  /// 调节特定频段增益 (索引, 增益dB)
  SetEqGain(usize, f32),
  /// 设置均衡器风格预设
  SetEqPreset(String),
  /// 触发本地音乐目录扫描
  ScanLocalFolder,
  /// 返回上一页
  GoBack,
  /// 前进下一页
  GoForward,
  /// 打开当前歌曲评论弹窗
  OpenComments,
  /// 关闭歌曲评论弹窗
  CloseComments,
  /// 导航至歌手详情页
  NavigateToArtist { id: String, name: String },
  /// 下载歌曲
  DownloadTrack(String),
  /// 切换本地/下载 Tab
  SwitchLibraryTab(String),
  /// 打开下载文件夹
  OpenDownloadFolder,
  /// 试试手气
  PlayRandomTrack,
  /// 收藏歌单/添加到我的歌单
  CollectPlaylist(String),
  /// 新建本地歌单
  CreateNewPlaylist,
  /// 复制当前歌词文本
  CopyCurrentLyric,
  /// 歌词时间轴手动微调偏移量 (秒)
  AdjustLyricOffset(f32),
}

/// 向 UI 推送的状态变更事件
#[derive(Debug, Clone)]
pub enum AppEvent {
  /// 播放器状态改变
  PlayerStateChanged(PlayerState),
  /// 播放进度推进
  ProgressUpdated { position_ms: u64, duration_ms: u64 },
  /// 当前播放歌曲变更
  TrackChanged(Option<Track>),
  /// 封面图片与提取代表色加载完成
  CoverLoaded {
    path: PathBuf,
    accent_color: slint::Color,
  },
  /// 歌单大封面加载完成
  PlaylistCoverLoaded(PathBuf),
  /// 主题主色已变更
  ThemeColorChanged(slint::Color),
  /// 主题十六进制色值已变更
  CustomColorChanged(String),
  /// 封面自适应动态取色开关变更
  DynamicColorChanged(bool),
  /// 深浅色外观模式已变更
  DarkModeChanged(bool),
  /// 全屏歌词浮层开关
  LyricViewToggled(bool),
  /// 播放队列抽屉开关
  QueueDrawerToggled(bool),
  /// 侧边导航栏折叠开关
  SidebarToggled(bool),
  /// 均衡器面板开关
  EqualizerToggled(bool),
  // TODO: preset_name 改为 Option<EqPreset>
  EqualizerChanged {
    preset_name: String,
    gains: Vec<f32>,
  },
  /// 设置弹窗开关
  SettingsToggled(bool),
  /// 登录弹窗开关
  LoginModalToggled(bool),
  /// 评论弹窗开关
  CommentsModalToggled(bool),
  /// 动态热搜榜已加载
  HotKeywordsLoaded(Vec<String>),
  /// 动态真实系统日期已获取
  SystemDateLoaded {
    day: String,
    month: String,
    weekday: String,
  },
  /// 歌词数据载入完成
  LyricsLoaded(Vec<LyricLine>),
  /// 歌词时间轴逐字与间奏推进
  LyricProgressUpdated {
    active_index: i32,
    word_progress: f32,
    is_interlude: bool,
    interlude_dots: i32,
  },
  /// 音量改变
  VolumeChanged(f32),
  /// 播放模式改变
  PlayModeChanged(PlayMode),
  /// 用户登录态改变
  UserChanged(Option<UserProfile>),
  /// 当前页面导航变更
  NavPageChanged(NavPage),
  /// 导航返回历史栈可用状态变更
  CanGoBackChanged(bool),
  /// 导航前进历史栈可用状态变更
  CanGoForwardChanged(bool),
  /// 当前播放歌曲喜欢状态变更
  TrackLikeStateChanged(bool),
  /// 特定歌曲收藏状态变更 (用于全局各个视图的红心图标实时响应)
  TrackLikeToggled { track_id: String, is_liked: bool },
  /// 弹出通知提示
  ShowToast(String),
  /// 歌单列表已加载
  PlaylistsLoaded(Vec<Playlist>),
  /// 每日推荐歌曲已加载
  DailySongsLoaded(Vec<Track>),
  /// 喜欢的歌曲已加载
  LikedSongsLoaded(Vec<Track>),
  /// 本地歌曲已更新
  LocalTracksLoaded(Vec<Track>),
  /// 32 频段音频动态频谱跳动更新
  VisualizerUpdated,
  /// 搜索结果已返回 (单曲与歌单)
  SearchResultsLoaded {
    songs: Vec<Track>,
    playlists: Vec<Playlist>,
  },
  /// 歌单详情曲目已加载
  PlaylistDetailLoaded {
    playlist: Playlist,
    tracks: Vec<Track>,
  },
  /// 播放队列变更
  QueueUpdated(Vec<Track>),
  /// 登录二维码已生成
  QrCodeGenerated { svg_data: String, key: String },
  /// 扫码状态变更文本
  QrStatusChanged(String),
  /// 音质标识更新
  QualityChanged(String),
  /// 推荐新歌已加载
  HomeRecommendSongsLoaded(Vec<Track>),
  /// 歌曲评论已加载
  CommentsLoaded(Vec<Comment>),
  /// 歌手详情与作品已加载
  ArtistLoaded {
    artist: Artist,
    desc: String,
    tracks: Vec<Track>,
  },
  /// 下载任务列表已更新
  DownloadsLoaded(Vec<DownloadTask>),
  /// 库管理 Tab 变更
  LibraryTabChanged(String),
  /// 最近播放足迹已加载
  RecentPlaysLoaded(Vec<Track>),
  /// 用户听歌统计与问候语加载完成
  ListeningStatsLoaded {
    greeting: String,
    stats: UserListeningStats,
  },
}
