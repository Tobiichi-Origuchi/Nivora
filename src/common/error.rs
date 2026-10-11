use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
  #[error("I/O 错误: {0}")]
  Io(#[from] std::io::Error),

  #[error("JSON 序列化/反序列化错误: {0}")]
  Json(#[from] serde_json::Error),

  #[error("网易云 API 错误: {0}")]
  NcmApi(#[from] ncm_api_rs::NcmError),

  #[error("网络请求错误: {0}")]
  Network(String),

  #[error("SQLite 数据库错误: {0}")]
  Sqlite(#[from] rusqlite::Error),

  #[error("数据库错误: {0}")]
  Database(String),

  #[error("音频引擎错误: {0}")]
  AudioEngine(String),

  #[error("歌词解析错误: {0}")]
  Lyric(String),

  #[error("未登录或会话已过期")]
  Unauthorized,

  #[error("资源未找到: {0}")]
  NotFound(String),

  #[error("系统配置错误: {0}")]
  Config(String),

  #[error("内部通道通信错误: {0}")]
  Channel(String),
}

pub type AppResult<T> = Result<T, AppError>;
