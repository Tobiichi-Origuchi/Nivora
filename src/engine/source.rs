use parking_lot::{Condvar, Mutex};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use symphonia::core::io::MediaSource;

use crate::common::error::{AppError, AppResult};

/// 渐进式流式缓冲区内部状态
pub struct ProgressiveBuffer {
  data: Vec<u8>,
  total_len: Option<u64>,
  is_complete: bool,
  is_cancelled: bool,
  error: Option<String>,
}

/// 渐进式网络音频流
#[derive(Clone)]
pub struct ProgressiveStream {
  shared: Arc<(Mutex<ProgressiveBuffer>, Condvar)>,
  pos: u64,
}

impl ProgressiveStream {
  pub fn new(total_len: Option<u64>) -> (Self, Arc<(Mutex<ProgressiveBuffer>, Condvar)>) {
    let shared = Arc::new((
      Mutex::new(ProgressiveBuffer {
        data: Vec::with_capacity(512 * 1024),
        total_len,
        is_complete: false,
        is_cancelled: false,
        error: None,
      }),
      Condvar::new(),
    ));
    let stream = Self {
      shared: shared.clone(),
      pos: 0,
    };
    (stream, shared)
  }

  /// 主动终止网络流拉取与等待
  pub fn cancel(&self) {
    let (lock, cvar) = &*self.shared;
    {
      let mut inner = lock.lock();
      inner.is_cancelled = true;
    }
    cvar.notify_all();
  }
}

impl Read for ProgressiveStream {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    if buf.is_empty() {
      return Ok(0);
    }
    let (lock, cvar) = &*self.shared;
    let mut inner = lock.lock();
    loop {
      if inner.is_cancelled {
        return Err(std::io::Error::new(
          std::io::ErrorKind::Interrupted,
          "流已被主动取消或切歌",
        ));
      }
      let available = inner.data.len();
      if (self.pos as usize) < available {
        let to_read = (available - self.pos as usize).min(buf.len());
        buf[..to_read].copy_from_slice(&inner.data[self.pos as usize..self.pos as usize + to_read]);
        self.pos += to_read as u64;
        return Ok(to_read);
      }
      if inner.is_complete {
        return Ok(0);
      }
      if let Some(err) = &inner.error {
        return Err(std::io::Error::other(err.clone()));
      }
      // 等待后续数据块到达
      let timeout = std::time::Duration::from_millis(500);
      let wait_res = cvar.wait_for(&mut inner, timeout);
      if wait_res.timed_out() {
        if inner.data.len() == available && inner.is_complete {
          return Ok(0);
        }
        if (self.pos as usize) > inner.data.len() + 10 * 1024 * 1024 && inner.is_complete {
          return Ok(0);
        }
      }
    }
  }
}

impl Seek for ProgressiveStream {
  fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
    let (lock, _) = &*self.shared;
    let inner = lock.lock();
    let new_pos = match pos {
      SeekFrom::Start(n) => n as i64,
      SeekFrom::Current(n) => self.pos as i64 + n,
      SeekFrom::End(n) => {
        if let Some(total) = inner.total_len {
          total as i64 + n
        } else {
          inner.data.len() as i64 + n
        }
      }
    };
    if new_pos < 0 {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "Seek before start",
      ));
    }
    self.pos = new_pos as u64;
    Ok(self.pos)
  }
}

impl MediaSource for ProgressiveStream {
  fn is_seekable(&self) -> bool {
    true
  }

  fn byte_len(&self) -> Option<u64> {
    let (lock, _) = &*self.shared;
    let inner = lock.lock();
    inner.total_len.or_else(|| {
      if inner.is_complete {
        Some(inner.data.len() as u64)
      } else {
        None
      }
    })
  }
}

/// 音源提供者抽象
pub enum AudioSourceStream {
  File(File),
  Memory(std::io::Cursor<Vec<u8>>),
  Progressive(ProgressiveStream),
}

impl Read for AudioSourceStream {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    match self {
      Self::File(f) => f.read(buf),
      Self::Memory(c) => c.read(buf),
      Self::Progressive(p) => p.read(buf),
    }
  }
}

impl Seek for AudioSourceStream {
  fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
    match self {
      Self::File(f) => f.seek(pos),
      Self::Memory(c) => c.seek(pos),
      Self::Progressive(p) => p.seek(pos),
    }
  }
}

impl MediaSource for AudioSourceStream {
  fn is_seekable(&self) -> bool {
    match self {
      Self::File(_) | Self::Memory(_) | Self::Progressive(_) => true,
    }
  }

  fn byte_len(&self) -> Option<u64> {
    match self {
      Self::File(f) => f.metadata().ok().map(|m| m.len()),
      Self::Memory(c) => Some(c.get_ref().len() as u64),
      Self::Progressive(p) => p.byte_len(),
    }
  }
}

impl AudioSourceStream {
  pub fn try_clone(&self) -> std::io::Result<Self> {
    match self {
      Self::File(f) => {
        let mut cloned = f.try_clone()?;
        let _ = cloned.seek(SeekFrom::Start(0));
        Ok(Self::File(cloned))
      }
      Self::Memory(c) => {
        let mut cloned = c.clone();
        cloned.set_position(0);
        Ok(Self::Memory(cloned))
      }
      Self::Progressive(p) => Ok(Self::Progressive(p.clone())),
    }
  }

  /// 主动终止网络流拉取
  pub fn cancel(&self) {
    if let Self::Progressive(p) = self {
      p.cancel();
    }
  }
}

/// 打开本地音频文件
pub fn open_file_source<P: AsRef<Path>>(path: P) -> AppResult<AudioSourceStream> {
  let file = File::open(path)?;
  Ok(AudioSourceStream::File(file))
}

/// 从内存字节缓冲区创建音源
pub fn open_memory_source(bytes: Vec<u8>) -> AudioSourceStream {
  AudioSourceStream::Memory(std::io::Cursor::new(bytes))
}

/// 拉取 HTTP 网络音频流到渐进式缓冲区
pub async fn fetch_http_stream(
  url: &str,
  cancel_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
) -> AppResult<AudioSourceStream> {
  let client = crate::common::http::HttpClientPool::get_client();

  let ip = crate::common::http::get_random_disguise_ip();
  let mut resp = client
    .get(url)
    .header("User-Agent", crate::common::http::DEFAULT_USER_AGENT)
    .header("X-Real-IP", ip)
    .header("X-Forwarded-For", ip)
    .send()
    .await
    .map_err(|e| AppError::Network(format!("网络请求失败: {e}")))?;

  if !resp.status().is_success() {
    return Err(AppError::Network(format!(
      "HTTP 状态码错误: {}",
      resp.status()
    )));
  }

  let total_len = resp.content_length();
  let (stream, shared) = ProgressiveStream::new(total_len);

  // 预读取首个 chunk 确保解码器探测头立即可用
  if let Ok(Some(first_chunk)) = resp.chunk().await {
    let (lock, cvar) = &*shared;
    {
      let mut inner = lock.lock();
      inner.data.extend_from_slice(&first_chunk);
    }
    cvar.notify_all();
  }

  // 拉取后续数据块并注入渐进缓冲区
  let cancel_inner = cancel_flag.clone();
  tokio::spawn(async move {
    let (lock, cvar) = &*shared;
    loop {
      if let Some(ref cf) = cancel_inner
        && cf.load(std::sync::atomic::Ordering::Relaxed)
      {
        let mut inner = lock.lock();
        inner.is_cancelled = true;
        cvar.notify_all();
        break;
      }
      {
        let inner = lock.lock();
        if inner.is_cancelled {
          break;
        }
      }
      match resp.chunk().await {
        Ok(Some(chunk)) => {
          {
            let mut inner = lock.lock();
            if inner.data.len() + chunk.len() > 128 * 1024 * 1024 {
              tracing::warn!("音频渐进缓冲区达到 128MB 上限，停止继续扩充");
              inner.is_complete = true;
              break;
            }
            inner.data.extend_from_slice(&chunk);
          }
          cvar.notify_all();
        }
        Ok(None) => {
          {
            let mut inner = lock.lock();
            inner.is_complete = true;
          }
          cvar.notify_all();
          break;
        }
        Err(e) => {
          {
            let mut inner = lock.lock();
            inner.error = Some(e.to_string());
            inner.is_complete = true;
          }
          cvar.notify_all();
          break;
        }
      }
    }
  });

  Ok(AudioSourceStream::Progressive(stream))
}
