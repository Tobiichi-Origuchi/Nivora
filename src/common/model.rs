use super::types::TrackSource;
use serde::{Deserialize, Serialize};

/// 歌手信息
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Artist {
  pub id: String,
  pub name: String,
  pub avatar: Option<String>,
}

/// 专辑信息
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Album {
  pub id: String,
  pub name: String,
  pub cover: Option<String>,
}

/// 歌曲音质详细元数据
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AudioQuality {
  pub sample_rate: u32,
  pub channels: u16,
  pub bit_rate: u32,
  pub codec: String,
}

/// 音频曲目
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Track {
  /// 唯一标识
  pub id: String,
  /// 标题
  pub title: String,
  /// 歌手列表
  pub artists: Vec<Artist>,
  /// 所属专辑
  pub album: Option<Album>,
  /// 时长
  pub duration_ms: u64,
  /// 封面图片链接/本地缓存路径
  pub cover_url: Option<String>,
  /// 高清封面链接
  #[serde(default)]
  pub cover_original: Option<String>,
  /// 曲目来源 (Ncm / Local)
  pub source: TrackSource,
  /// 本地文件路径
  pub local_path: Option<String>,
  /// 付费/VIP 标记 (0: 免费, 1: VIP, 4: 购买专辑)
  pub fee: i32,
}

impl Track {
  /// 获取以逗号分隔的艺术家名称
  pub fn artists_text(&self) -> String {
    if self.artists.is_empty() {
      "未知歌手".to_string()
    } else {
      self
        .artists
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(" / ")
    }
  }
}

/// 歌单
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Playlist {
  pub id: String,
  pub title: String,
  pub cover_url: Option<String>,
  pub description: Option<String>,
  pub track_count: usize,
  pub creator_name: Option<String>,
  pub play_count: u64,
}

/// 用户资料
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UserProfile {
  pub user_id: i64,
  pub nickname: String,
  pub avatar_url: String,
  pub vip_type: i32,
  pub is_anonymous: bool,
}

/// 播放历史记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayHistoryItem {
  pub id: i64,
  pub track_id: String,
  pub source: String,
  pub started_at: i64,
  pub listened_ms: u64,
  pub track: Track,
}

/// 收藏状态历史记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FavoriteItem {
  pub id: i64,
  pub track_id: String,
  pub source: String,
  pub action: String,
  pub at: i64,
  pub track: Track,
}

/// 离线下载任务
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadTask {
  pub task_id: String,
  pub track: Track,
  pub quality_level: String,
  pub status: String,
  pub received: u64,
  pub total: u64,
  pub file_path: Option<String>,
  pub error_code: Option<String>,
  pub tag_warning: bool,
  pub created_at: i64,
  pub finished_at: Option<i64>,
}

/// 歌曲评论
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Comment {
  pub id: i64,
  pub user_nickname: String,
  pub user_avatar: String,
  pub content: String,
  pub time_str: String,
  pub liked_count: i32,
}

/// 用户听歌行为统计指标
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UserListeningStats {
  pub weekly_hours_str: String,
  pub total_tracks_played: u64,
  pub streak_days: u32,
  pub total_favorites: u64,
  pub weekly_growth_str: String,
  pub favorite_ratio_str: String,
}
