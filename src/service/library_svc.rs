use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::common::error::AppResult;
use crate::common::model::{Album, Artist, Track};
use crate::common::types::TrackSource;
use crate::data::db::Database;
use crate::data::repo_track::TrackRepo;

/// 本地音频文件后缀白名单
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "ogg", "m4a", "aac", "alac"];

/// 本地音乐库扫描与元数据管理服务
pub struct LibraryService;

impl LibraryService {
  /// 检查指定路径是否为音频文件
  pub fn is_audio_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
      let lower = ext.to_ascii_lowercase();
      AUDIO_EXTENSIONS.contains(&lower.as_str())
    } else {
      false
    }
  }

  /// 递归扫描指定目录下的音频文件并提取元数据
  pub fn scan_directory<P: AsRef<Path>>(db: &Database, dir_path: P) -> AppResult<usize> {
    let dir = dir_path.as_ref();
    if !dir.exists() || !dir.is_dir() {
      return Ok(0);
    }

    let covers_dir = crate::common::config::AppConfig::cache_dir()
      .unwrap_or_else(|_| PathBuf::from("/tmp/nivora_cache"))
      .join("covers");
    let _ = fs::create_dir_all(&covers_dir);

    let mut scanned_count = 0;

    for entry in WalkDir::new(dir)
      .follow_links(true)
      .into_iter()
      .filter_map(|e| e.ok())
    {
      let path = entry.path();
      if !path.is_file() || !Self::is_audio_file(path) {
        continue;
      }

      if let Ok(metadata) = fs::metadata(path) {
        let file_size = metadata.len() as i64;
        let path_str = path.to_string_lossy().to_string();
        let stem = path
          .file_stem()
          .and_then(|s| s.to_str())
          .unwrap_or("未知曲目");
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("mp3");

        // 读取音频标签与时长/码率
        let (title, artist, album, duration_ms, sample_rate, bitrate, cover_path) =
          match Probe::open(path).and_then(|p| p.read()) {
            Ok(tagged_file) => {
              let props = tagged_file.properties();
              let dur = props.duration().as_millis() as u64;
              let sr = props.sample_rate();
              let br = props.audio_bitrate();

              let tag_opt = tagged_file
                .primary_tag()
                .or_else(|| tagged_file.first_tag());

              let t = tag_opt
                .and_then(|t| t.title().as_deref().map(|s| s.to_string()))
                .unwrap_or_else(|| stem.to_string());
              let a = tag_opt
                .and_then(|t| t.artist().as_deref().map(|s| s.to_string()))
                .unwrap_or_else(|| "未知歌手".to_string());
              let alb = tag_opt
                .and_then(|t| t.album().as_deref().map(|s| s.to_string()))
                .unwrap_or_else(|| "本地音频".to_string());

              // 提取专辑封面并缓存至磁盘
              let mut cov = None;
              if let Some(tag) = tag_opt
                && let Some(pic) = tag.pictures().first()
              {
                let ext = crate::service::CoverService::sniff_image_ext(pic.data());
                let pic_hash = format!("{:016x}", hash_bytes_u64(pic.data()));
                let cov_file = covers_dir.join(format!("{}.{}", pic_hash, ext));
                if !cov_file.exists() {
                  let _ = fs::write(&cov_file, pic.data());
                }
                cov = Some(cov_file.to_string_lossy().to_string());
              }

              (t, a, alb, dur, sr, br, cov)
            }
            Err(_) => {
              // 降级使用文件名推断
              let (artist_part, title_part) = if let Some((a, t)) = stem.split_once(" - ") {
                (a.trim().to_string(), t.trim().to_string())
              } else {
                ("未知歌手".to_string(), stem.trim().to_string())
              };
              (
                title_part,
                artist_part,
                "本地音频".to_string(),
                0,
                None,
                None,
                None,
              )
            }
          };

        let track_id = format!("local:{}", path_str);
        let track = Track {
          id: track_id,
          title,
          artists: vec![Artist {
            id: String::new(),
            name: artist,
            avatar: None,
          }],
          album: Some(Album {
            id: String::new(),
            name: album,
            cover: cover_path.clone(),
          }),
          duration_ms,
          cover_url: cover_path,
          cover_original: None,
          source: TrackSource::Local,
          local_path: Some(path_str),
          fee: 0,
        };

        if let Err(e) =
          TrackRepo::upsert_track(db, &track, file_size, Some(ext), sample_rate, bitrate)
        {
          tracing::warn!("本地音频元数据入库失败 (曲目: {}): {e}", track.title);
        } else {
          scanned_count += 1;
        }
      }
    }

    Ok(scanned_count)
  }

  /// 获取数据库中所有已收录的本地音乐列表
  pub fn get_all_local_tracks(db: &Database) -> AppResult<Vec<Track>> {
    TrackRepo::list_tracks(db)
  }

  /// 本地音乐库领域动作统一调度入口
  pub fn dispatch(
    db: Database,
    config: std::sync::Arc<parking_lot::Mutex<crate::common::config::AppConfig>>,
    ev_tx: tokio::sync::mpsc::UnboundedSender<crate::common::channel::AppEvent>,
    action: crate::common::channel::UiAction,
  ) {
    match action {
      crate::common::channel::UiAction::ScanLocalFolder => {
        Self::scan_local_folder_interactive(db, config, ev_tx);
      }
      crate::common::channel::UiAction::SwitchLibraryTab(tab) => {
        Self::switch_tab(&db, &ev_tx, tab);
      }
      _ => {}
    }
  }

  /// 触发本地音乐目录选择与扫描
  pub fn scan_local_folder_interactive(
    db: Database,
    config: std::sync::Arc<parking_lot::Mutex<crate::common::config::AppConfig>>,
    ev_tx: tokio::sync::mpsc::UnboundedSender<crate::common::channel::AppEvent>,
  ) {
    tokio::spawn(async move {
      let selected = rfd::AsyncFileDialog::new()
        .set_title("选择本地音乐扫描文件夹")
        .pick_folder()
        .await;

      let target_dir = if let Some(handle) = selected {
        let dir = handle.path().to_path_buf();
        let mut cfg = config.lock();
        if !cfg.music_folders.contains(&dir) {
          cfg.music_folders.push(dir.clone());
          let _ = cfg.save();
        }
        dir
      } else {
        let cfg = config.lock();
        cfg.music_folders.first().cloned().unwrap_or_else(|| {
          directories_next::UserDirs::new()
            .and_then(|u| u.audio_dir().map(|p| p.to_path_buf()))
            .or_else(|| directories_next::BaseDirs::new().map(|b| b.home_dir().join("Music")))
            .unwrap_or_else(|| PathBuf::from("Music"))
        })
      };

      tokio::task::spawn_blocking(move || {
        let _ = Self::scan_directory(&db, target_dir);
        if let Ok(tracks) = Self::get_all_local_tracks(&db) {
          let _ = ev_tx.send(crate::common::channel::AppEvent::LocalTracksLoaded(tracks));
        }
      });
    });
  }

  /// 切换音乐库 Tab
  pub fn switch_tab(
    db: &Database,
    event_tx: &tokio::sync::mpsc::UnboundedSender<crate::common::channel::AppEvent>,
    tab: String,
  ) {
    let _ = event_tx.send(crate::common::channel::AppEvent::LibraryTabChanged(
      tab.clone(),
    ));
    if tab == "download"
      && let Ok(tasks) = crate::service::DownloadService::list_tasks(db)
    {
      let _ = event_tx.send(crate::common::channel::AppEvent::DownloadsLoaded(tasks));
    }
  }
}

/// 计算字节切片的 64 位哈希值 (基于快速 DefaultHasher)
fn hash_bytes_u64(data: &[u8]) -> u64 {
  use std::collections::hash_map::DefaultHasher;
  use std::hash::Hasher;
  let mut s = DefaultHasher::new();
  s.write(data);
  s.finish()
}
