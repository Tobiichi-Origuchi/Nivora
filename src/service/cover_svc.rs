use parking_lot::RwLock;
use slint::Color;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tokio::sync::mpsc;

use crate::common::channel::AppEvent;
use crate::common::model::{Artist, Playlist, Track};
use crate::common::state::AppState;
use crate::service::ThemeService;

const MAX_UI_IMAGE_CACHE_CAPACITY: usize = 1024;

struct BoundedImageCache {
  map: HashMap<String, slint::Image>,
  order: VecDeque<String>,
}

impl BoundedImageCache {
  fn new() -> Self {
    Self {
      map: HashMap::new(),
      order: VecDeque::new(),
    }
  }

  fn get(&mut self, key: &str) -> Option<slint::Image> {
    let img = self.map.get(key)?.clone();
    if let Some(pos) = self.order.iter().position(|k| k == key) {
      self.order.remove(pos);
      self.order.push_back(key.to_string());
    }
    Some(img)
  }

  fn insert(&mut self, key: String, img: slint::Image) {
    if self.map.contains_key(&key) {
      self.map.insert(key.clone(), img);
      if let Some(pos) = self.order.iter().position(|k| k == &key) {
        self.order.remove(pos);
      }
      self.order.push_back(key);
      return;
    }

    while self.map.len() >= MAX_UI_IMAGE_CACHE_CAPACITY {
      if let Some(oldest) = self.order.pop_front() {
        self.map.remove(&oldest);
      } else {
        break;
      }
    }

    self.map.insert(key.clone(), img);
    self.order.push_back(key);
  }
}

thread_local! {
  static UI_IMAGE_CACHE: RefCell<BoundedImageCache> =
    RefCell::new(BoundedImageCache::new());
}

const MAX_SHARED_PIXEL_CACHE_CAPACITY: usize = 512;

pub struct BoundedPixelCache {
  map: HashMap<String, slint::SharedPixelBuffer<slint::Rgba8Pixel>>,
  order: VecDeque<String>,
}

impl BoundedPixelCache {
  fn new() -> Self {
    Self {
      map: HashMap::new(),
      order: VecDeque::new(),
    }
  }

  pub fn get(&mut self, key: &str) -> Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
    let buf = self.map.get(key)?.clone();
    if let Some(pos) = self.order.iter().position(|k| k == key) {
      self.order.remove(pos);
      self.order.push_back(key.to_string());
    }
    Some(buf)
  }

  pub fn insert(&mut self, key: String, buf: slint::SharedPixelBuffer<slint::Rgba8Pixel>) {
    if self.map.contains_key(&key) {
      self.map.insert(key.clone(), buf);
      if let Some(pos) = self.order.iter().position(|k| k == &key) {
        self.order.remove(pos);
      }
      self.order.push_back(key);
      return;
    }

    while self.map.len() >= MAX_SHARED_PIXEL_CACHE_CAPACITY {
      if let Some(oldest) = self.order.pop_front() {
        self.map.remove(&oldest);
      } else {
        break;
      }
    }

    self.map.insert(key.clone(), buf);
    self.order.push_back(key);
  }
}

static SHARED_PIXEL_CACHE: OnceLock<RwLock<BoundedPixelCache>> = OnceLock::new();

/// 封面加载与缓存服务
pub struct CoverService;

impl CoverService {
  /// 规范化网络封面 URL，确保默认附带 300 尺寸参数以匹配持久化缓存哈希
  pub fn normalize_url(cover_url_or_path: &str) -> Cow<'_, str> {
    if (cover_url_or_path.starts_with("http://") || cover_url_or_path.starts_with("https://"))
      && !cover_url_or_path.contains("param=")
      && let Some(sized) = crate::common::http::with_pic_size(Some(cover_url_or_path), 300)
    {
      return Cow::Owned(sized);
    }
    Cow::Borrowed(cover_url_or_path)
  }

  /// 封面持久化缓存目录与基础哈希值 (缓存目录仅创建一次，零重复系统调用)
  fn cover_cache_base(cover_url_or_path: &str) -> (&'static Path, u64) {
    static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
    let cache_dir = CACHE_DIR.get_or_init(|| {
      let dir = crate::common::config::AppConfig::cache_dir()
        .map(|d| d.join("covers"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/nivora_covers"));
      let _ = std::fs::create_dir_all(&dir);
      dir
    });

    let normalized = Self::normalize_url(cover_url_or_path);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(normalized.as_ref(), &mut hasher);
    let hash_val = std::hash::Hasher::finish(&hasher);
    (cache_dir.as_path(), hash_val)
  }

  /// 根据图片二进制魔数嗅探真实文件扩展名 (严格区分 PNG / JPEG)
  pub fn sniff_image_ext(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
      "png"
    } else {
      "jpg"
    }
  }

  /// 获取指定扩展名的封面缓存路径
  pub fn get_cached_cover_path_for_ext(cover_url_or_path: &str, ext: &str) -> PathBuf {
    let (cache_dir, hash_val) = Self::cover_cache_base(cover_url_or_path);
    cache_dir.join(format!("{:016x}.{}", hash_val, ext))
  }

  /// 查找已存在的封面缓存路径（支持 .png 与 .jpg）
  pub fn find_cached_cover(cover_url_or_path: &str) -> Option<PathBuf> {
    let (cache_dir, hash_val) = Self::cover_cache_base(cover_url_or_path);
    let png = cache_dir.join(format!("{:016x}.png", hash_val));
    if png.is_file() && png.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
      return Some(png);
    }
    let jpg = cache_dir.join(format!("{:016x}.jpg", hash_val));
    if jpg.is_file() && jpg.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
      return Some(jpg);
    }
    None
  }

  /// 计算给定封面 URL 或路径的默认本地持久化缓存文件路径
  pub fn get_cached_cover_path(cover_url_or_path: &str) -> PathBuf {
    Self::find_cached_cover(cover_url_or_path)
      .unwrap_or_else(|| Self::get_cached_cover_path_for_ext(cover_url_or_path, "jpg"))
  }

  /// 快速查找封面图像：优先纯内存哈希极速返回
  pub fn try_get_cached_image(cover_url_or_path: &str) -> (slint::Image, bool) {
    Self::try_get_cached_image_limited(cover_url_or_path, true)
  }

  pub fn get_shared_pixel_cache() -> &'static RwLock<BoundedPixelCache> {
    SHARED_PIXEL_CACHE.get_or_init(|| RwLock::new(BoundedPixelCache::new()))
  }

  /// 在后台线程解压图片并存入跨线程共享像素缓冲，使 UI 线程零耗时显示
  pub fn decode_and_cache_pixels(key: &str, bytes: &[u8]) {
    let normalized = Self::normalize_url(key);
    let norm_key = normalized.as_ref();
    if Self::get_shared_pixel_cache()
      .read()
      .map
      .contains_key(norm_key)
    {
      return;
    }
    if let Ok(dyn_img) = image::load_from_memory(bytes) {
      let rgba = dyn_img.to_rgba8();
      let (w, h) = (rgba.width(), rgba.height());
      let mut pixel_buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
      pixel_buf.make_mut_bytes().copy_from_slice(rgba.as_raw());
      Self::get_shared_pixel_cache()
        .write()
        .insert(norm_key.to_string(), pixel_buf);
    }
  }

  /// 快速查找封面图像：支持限制是否从磁盘同步解码（首屏之外不读盘，杜绝卡死）
  pub fn try_get_cached_image_limited(
    cover_url_or_path: &str,
    allow_disk_decode: bool,
  ) -> (slint::Image, bool) {
    if cover_url_or_path.is_empty() {
      return (slint::Image::default(), false);
    }

    let normalized = Self::normalize_url(cover_url_or_path);
    let norm_key = normalized.as_ref();

    // 1. 优先查 UI 线程内存缓存 (零 I/O 极速命中)
    let cached = UI_IMAGE_CACHE.with_borrow_mut(|cache| cache.get(norm_key));
    if let Some(img) = cached {
      return (img, true);
    }

    // 2. 查后台工作线程已解码的像素缓冲 (零 I/O、零 JPEG 解压消耗，瞬间转化为 slint::Image)
    if let Some(pixel_buf) = Self::get_shared_pixel_cache().write().get(norm_key) {
      let img = slint::Image::from_rgba8(pixel_buf);
      Self::insert_cached_image(norm_key, img.clone());
      return (img, true);
    }

    if !allow_disk_decode {
      return (slint::Image::default(), false);
    }

    // 3. 仅对本地非 HTTP 路径做文件检查 (网络 URL 绝不做 is_file 错误系统调用)
    if !cover_url_or_path.starts_with("http://") && !cover_url_or_path.starts_with("https://") {
      let direct_path = Path::new(cover_url_or_path);
      if direct_path.is_file()
        && let Ok(img) = slint::Image::load_from_path(direct_path)
      {
        Self::insert_cached_image(norm_key, img.clone());
        return (img, true);
      }
    }

    // 4. 检查持久化封面缓存目录中的已落盘文件
    if let Some(cache_path) = Self::find_cached_cover(norm_key)
      && let Ok(img) = slint::Image::load_from_path(&cache_path)
    {
      Self::insert_cached_image(norm_key, img.clone());
      return (img, true);
    }

    (slint::Image::default(), false)
  }

  /// 将已解码的封面存入 UI 线程内存缓存
  pub fn insert_cached_image(key: &str, img: slint::Image) {
    let normalized = Self::normalize_url(key);
    UI_IMAGE_CACHE.with_borrow_mut(|cache| {
      cache.insert(normalized.to_string(), img);
    });
  }

  /// 异步拉取网络或本地图片，写入本地缓存并提取自适应代表色 (返回缓存路径与提取颜色)
  pub async fn load_cover(cover_url_or_path: &str, track_id: &str) -> Option<(PathBuf, Color)> {
    if cover_url_or_path.is_empty() {
      return None;
    }

    let normalized = Self::normalize_url(cover_url_or_path);
    let target_ref = normalized.as_ref();

    // 1. 若已有有效缓存文件，直接复用 (无需重复下载与写盘，进入后台阻塞线程执行读取与解码)
    if let Some(cache_path) = Self::find_cached_cover(target_ref) {
      let cpath = cache_path.clone();
      let tid = track_id.to_string();
      let target_str = target_ref.to_string();
      return tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&cpath).ok()?;
        let color = ThemeService::extract_cover_accent(&tid, Some(&bytes));
        Self::decode_and_cache_pixels(&target_str, &bytes);
        Some((cpath, color))
      })
      .await
      .ok()
      .flatten();
    }

    // 2. 拉取数据 (网络或本地原文件)
    let bytes = if target_ref.starts_with("http://") || target_ref.starts_with("https://") {
      let client = crate::common::http::HttpClientPool::get_client();
      let resp = client.get(target_ref).send().await.ok()?;
      if !resp.status().is_success() {
        return None;
      }
      let data = resp.bytes().await.ok()?.to_vec();
      if data.is_empty() {
        return None;
      }
      data
    } else if Path::new(target_ref).is_file() {
      let file_path = target_ref.to_string();
      let data = tokio::task::spawn_blocking(move || std::fs::read(&file_path).ok())
        .await
        .ok()
        .flatten()?;
      if data.is_empty() {
        return None;
      }
      data
    } else {
      return None;
    };

    // 3. 严格根据二进制魔数写入正确扩展名 (.png 或 .jpg) 并提取色系 (进入后台阻塞任务)
    let target_str = target_ref.to_string();
    let tid = track_id.to_string();
    tokio::task::spawn_blocking(move || {
      let ext = Self::sniff_image_ext(&bytes);
      let cache_path = Self::get_cached_cover_path_for_ext(&target_str, ext);
      let _ = std::fs::write(&cache_path, &bytes);

      let color = ThemeService::extract_cover_accent(&tid, Some(&bytes));
      Self::decode_and_cache_pixels(&target_str, &bytes);
      Some((cache_path, color))
    })
    .await
    .ok()
    .flatten()
  }

  /// 并发拉取歌单封面并在落盘后通知 UI 刷新
  pub fn spawn_playlist_covers_loader(
    playlists: Vec<Playlist>,
    ev_tx: mpsc::UnboundedSender<AppEvent>,
  ) {
    tokio::spawn(async move {
      let mut set = tokio::task::JoinSet::new();
      let sem = Arc::new(tokio::sync::Semaphore::new(8));

      for pl in &playlists {
        if let Some(url) = pl.cover_url.clone()
          && !url.is_empty()
        {
          let key = format!("pl_{}", pl.id);
          let permit_sem = sem.clone();
          set.spawn(async move {
            let _permit = permit_sem.acquire().await.ok();
            Self::load_cover(&url, &key).await
          });
        }
      }

      let mut pending = 0;
      let mut last_flush = std::time::Instant::now();

      while let Some(res) = set.join_next().await {
        if let Ok(Some(_)) = res {
          pending += 1;
          let elapsed = last_flush.elapsed();
          if (pending >= 6 || elapsed >= std::time::Duration::from_millis(250)) && pending > 0 {
            pending = 0;
            last_flush = std::time::Instant::now();
            let _ = ev_tx.send(AppEvent::PlaylistsLoaded(playlists.clone()));
          }
        }
      }
      if pending > 0 {
        let _ = ev_tx.send(AppEvent::PlaylistsLoaded(playlists));
      }
    });
  }

  /// 并发拉取歌曲列表封面并在落盘后通知 UI 刷新
  pub fn spawn_tracks_covers_loader<F>(
    tracks: Vec<Track>,
    max_count: usize,
    ev_tx: mpsc::UnboundedSender<AppEvent>,
    event_builder: F,
  ) where
    F: Fn(Vec<Track>) -> AppEvent + Send + 'static,
  {
    tokio::spawn(async move {
      let mut set = tokio::task::JoinSet::new();
      let sem = Arc::new(tokio::sync::Semaphore::new(8));

      for track in tracks.iter().take(max_count) {
        let cover_opt = track
          .cover_url
          .clone()
          .or_else(|| track.album.as_ref().and_then(|a| a.cover.clone()));
        if let Some(url) = cover_opt
          && !url.is_empty()
        {
          let key = track.id.clone();
          let permit_sem = sem.clone();
          set.spawn(async move {
            let _permit = permit_sem.acquire().await.ok();
            Self::load_cover(&url, &key).await
          });
        }
      }

      let mut pending = 0;
      let mut last_flush = std::time::Instant::now();

      while let Some(res) = set.join_next().await {
        if let Ok(Some(_)) = res {
          pending += 1;
          let elapsed = last_flush.elapsed();
          if (pending >= 8 || elapsed >= std::time::Duration::from_millis(250)) && pending > 0 {
            pending = 0;
            last_flush = std::time::Instant::now();
            let _ = ev_tx.send(event_builder(tracks.clone()));
          }
        }
      }
      if pending > 0 {
        let _ = ev_tx.send(event_builder(tracks));
      }
    });
  }

  /// 并发拉取歌单详情曲目的封面并在落盘后通知 UI 刷新
  pub fn spawn_playlist_detail_covers_loader(
    playlist: Playlist,
    tracks: Vec<Track>,
    max_count: usize,
    ev_tx: mpsc::UnboundedSender<AppEvent>,
    state: Arc<RwLock<AppState>>,
  ) {
    tokio::spawn(async move {
      let mut set = tokio::task::JoinSet::new();
      let sem = Arc::new(tokio::sync::Semaphore::new(8));

      for track in tracks.iter().take(max_count) {
        let cover_opt = track
          .cover_url
          .clone()
          .or_else(|| track.album.as_ref().and_then(|a| a.cover.clone()));
        if let Some(url) = cover_opt
          && !url.is_empty()
        {
          let key = track.id.clone();
          let permit_sem = sem.clone();
          set.spawn(async move {
            let _permit = permit_sem.acquire().await.ok();
            Self::load_cover(&url, &key).await
          });
        }
      }

      let playlist_id = playlist.id.clone();
      let mut pending = 0;
      let mut last_flush = std::time::Instant::now();

      while let Some(res) = set.join_next().await {
        if state.read().current_playlist_id.as_deref() != Some(&playlist_id) {
          return;
        }
        if let Ok(Some(_)) = res {
          pending += 1;
          let elapsed = last_flush.elapsed();
          if (pending >= 8 || elapsed >= std::time::Duration::from_millis(250)) && pending > 0 {
            pending = 0;
            last_flush = std::time::Instant::now();
            let _ = ev_tx.send(AppEvent::PlaylistDetailLoaded {
              playlist: playlist.clone(),
              tracks: tracks.clone(),
            });
          }
        }
      }
      if pending > 0 && state.read().current_playlist_id.as_deref() == Some(&playlist_id) {
        let _ = ev_tx.send(AppEvent::PlaylistDetailLoaded { playlist, tracks });
      }
    });
  }

  /// 并发拉取歌手热门作品的封面并在落盘后通知 UI 刷新
  pub fn spawn_artist_covers_loader(
    artist: Artist,
    desc: String,
    tracks: Vec<Track>,
    max_count: usize,
    ev_tx: mpsc::UnboundedSender<AppEvent>,
    state: Arc<RwLock<AppState>>,
  ) {
    tokio::spawn(async move {
      let mut set = tokio::task::JoinSet::new();
      let sem = Arc::new(tokio::sync::Semaphore::new(8));

      for track in tracks.iter().take(max_count) {
        let cover_opt = track
          .cover_url
          .clone()
          .or_else(|| track.album.as_ref().and_then(|a| a.cover.clone()));
        if let Some(url) = cover_opt
          && !url.is_empty()
        {
          let key = track.id.clone();
          let permit_sem = sem.clone();
          set.spawn(async move {
            let _permit = permit_sem.acquire().await.ok();
            Self::load_cover(&url, &key).await
          });
        }
      }

      let artist_id = artist.id.clone();
      let mut pending = 0;
      let mut last_flush = std::time::Instant::now();

      while let Some(res) = set.join_next().await {
        if state.read().current_artist_id.as_deref() != Some(&artist_id) {
          return;
        }
        if let Ok(Some(_)) = res {
          pending += 1;
          let elapsed = last_flush.elapsed();
          if (pending >= 8 || elapsed >= std::time::Duration::from_millis(250)) && pending > 0 {
            pending = 0;
            last_flush = std::time::Instant::now();
            let _ = ev_tx.send(AppEvent::ArtistLoaded {
              artist: artist.clone(),
              desc: desc.clone(),
              tracks: tracks.clone(),
            });
          }
        }
      }
      if pending > 0 && state.read().current_artist_id.as_deref() == Some(&artist_id) {
        let _ = ev_tx.send(AppEvent::ArtistLoaded {
          artist,
          desc,
          tracks,
        });
      }
    });
  }

  /// 并发拉取搜索结果的封面并在落盘后通知 UI 刷新
  pub fn spawn_search_covers_loader(
    songs: Vec<Track>,
    playlists: Vec<Playlist>,
    ev_tx: mpsc::UnboundedSender<AppEvent>,
  ) {
    tokio::spawn(async move {
      let mut set = tokio::task::JoinSet::new();
      let sem = Arc::new(tokio::sync::Semaphore::new(8));

      for pl in &playlists {
        if let Some(url) = pl.cover_url.clone()
          && !url.is_empty()
        {
          let key = format!("pl_{}", pl.id);
          let permit_sem = sem.clone();
          set.spawn(async move {
            let _permit = permit_sem.acquire().await.ok();
            Self::load_cover(&url, &key).await
          });
        }
      }
      for track in &songs {
        if let Some(url) = track.cover_url.clone()
          && !url.is_empty()
        {
          let key = track.id.clone();
          let permit_sem = sem.clone();
          set.spawn(async move {
            let _permit = permit_sem.acquire().await.ok();
            Self::load_cover(&url, &key).await
          });
        }
      }

      let mut pending = 0;
      let mut last_flush = std::time::Instant::now();

      while let Some(res) = set.join_next().await {
        if let Ok(Some(_)) = res {
          pending += 1;
          let elapsed = last_flush.elapsed();
          if (pending >= 6 || elapsed >= std::time::Duration::from_millis(250)) && pending > 0 {
            pending = 0;
            last_flush = std::time::Instant::now();
            let _ = ev_tx.send(AppEvent::SearchResultsLoaded {
              songs: songs.clone(),
              playlists: playlists.clone(),
            });
          }
        }
      }
      if pending > 0 {
        let _ = ev_tx.send(AppEvent::SearchResultsLoaded { songs, playlists });
      }
    });
  }
}
