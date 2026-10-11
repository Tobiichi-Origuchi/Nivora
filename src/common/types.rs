use serde::{Deserialize, Serialize};

/// 播放器运行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlayerState {
  #[default]
  Idle,
  Loading,
  Playing,
  Paused,
  Stopped,
  Error,
}

/// 播放循环模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlayMode {
  #[default]
  Sequence,
  RepeatOne,
  RepeatAll,
  Shuffle,
}

/// 音频曲目来源
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TrackSource {
  #[default]
  Ncm,
  Local,
}

/// 音频品质偏好等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioQualityLevel {
  Standard, // 128k MP3
  Higher,   // 192k MP3
  #[default]
  ExHigh, // 320k MP3
  Lossless, // SQ / FLAC
  HiRes,    // 24bit Hi-Res
}

impl AudioQualityLevel {
  pub fn bitrate_hint(&self) -> i64 {
    match self {
      Self::Standard => 128_000,
      Self::Higher => 192_000,
      Self::ExHigh => 320_000,
      Self::Lossless => 999_000,
      Self::HiRes => 1_999_000,
    }
  }

  pub fn as_level_str(&self) -> &'static str {
    match self {
      Self::Standard => "standard",
      Self::Higher => "higher",
      Self::ExHigh => "exhigh",
      Self::Lossless => "lossless",
      Self::HiRes => "hires",
    }
  }
}

/// 主界面导航活动页面枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NavPage {
  #[default]
  Home,
  Daily,
  Liked,
  Playlist,
  Library,
  Search,
  Artist,
  Stats,
}
