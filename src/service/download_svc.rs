use lofty::file::TaggedFileExt;
use lofty::picture::{Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::{Accessor, Tag, TagExt};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::api::client::NcmClient;
use crate::common::channel::{AppEvent, UiAction};
use crate::common::error::{AppError, AppResult};
use crate::common::model::{DownloadTask, Track};
use crate::common::state::AppState;
use crate::data::db::Database;
use crate::data::repo_download::DownloadRepo;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// 音乐文件下载与元数据标签内嵌服务
pub struct DownloadService;

impl DownloadService {
  /// 查询所有离线下载任务
  pub fn list_tasks(db: &Database) -> AppResult<Vec<DownloadTask>> {
    DownloadRepo::list_tasks(db)
  }

  /// 获取默认下载目录
  pub fn default_download_dir() -> PathBuf {
    crate::common::config::AppConfig::data_dir()
      .unwrap_or_else(|_| PathBuf::from("./data"))
      .join("downloads")
  }

  /// 执行曲目完整下载并嵌入 ID3 / Vorbis 标签与封面
  pub async fn download_track(
    db: &Database,
    track: Track,
    quality: &str,
    target_dir: &Path,
    ncm: &NcmClient,
  ) -> AppResult<PathBuf> {
    let task_id = format!("dl_{}_{}", track.id, quality);

    // 1. 在数据库中注册下载任务
    let task = DownloadTask {
      task_id: task_id.clone(),
      track: track.clone(),
      quality_level: quality.to_string(),
      status: "pending".to_string(),
      received: 0,
      total: 0,
      file_path: None,
      error_code: None,
      tag_warning: false,
      created_at: 0,
      finished_at: None,
    };
    DownloadRepo::create_task(db, &task)?;

    // 2. 获取下载直链
    let q = match quality {
      "standard" => crate::common::types::AudioQualityLevel::Standard,
      "higher" => crate::common::types::AudioQualityLevel::Higher,
      "lossless" => crate::common::types::AudioQualityLevel::Lossless,
      "hires" => crate::common::types::AudioQualityLevel::HiRes,
      _ => crate::common::types::AudioQualityLevel::ExHigh,
    };
    let song_url = match ncm.song().get_song_url(&track.id, q).await {
      Ok(Some(url)) => url,
      Ok(None) => {
        let err = "未能获取到该音质的播放/下载直链";
        if let Err(e) = DownloadRepo::fail_task(db, &task_id, err) {
          tracing::warn!("更新任务失败状态失败 (task: {task_id}): {e}");
        }
        return Err(AppError::Network(err.to_string()));
      }
      Err(e) => {
        let err_str = e.to_string();
        if let Err(e_repo) = DownloadRepo::fail_task(db, &task_id, &err_str) {
          tracing::warn!("更新任务失败状态失败 (task: {task_id}): {e_repo}");
        }
        return Err(e);
      }
    };

    // 3. 准备文件路径
    std::fs::create_dir_all(target_dir)?;
    let ext = if song_url.contains(".flac") {
      "flac"
    } else {
      "mp3"
    };
    let safe_title = track
      .title
      .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let safe_artist = track
      .artists_text()
      .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let filename = format!("{} - {}.{}", safe_artist, safe_title, ext);
    let dest_path = target_dir.join(&filename);

    // 4. 发起下载并更新进度
    let client = crate::common::http::HttpClientPool::get_client();
    let mut resp = client
      .get(&song_url)
      .send()
      .await
      .map_err(|e| AppError::Network(format!("下载网络请求失败: {e}")))?;

    let total_size = resp.content_length().unwrap_or(0);
    let mut file = File::create(&dest_path)?;
    let mut received = 0u64;

    while let Ok(Some(chunk)) = resp.chunk().await {
      file.write_all(&chunk)?;
      received += chunk.len() as u64;
      let _ = DownloadRepo::update_progress(db, &task_id, received, total_size);
    }
    file.flush()?;

    // 5. 拉取封面图片
    let mut cover_bytes = Vec::new();
    if let Some(ref cover_url) = track.cover_url
      && let Ok(c_resp) = client.get(cover_url).send().await
      && let Ok(b) = c_resp.bytes().await
    {
      cover_bytes = b.to_vec();
    }

    // 6. 将元数据与封面写入音频文件
    let tag_res = Self::write_metadata_to_file(&dest_path, &track, &cover_bytes);
    let tag_warning = tag_res.is_err();

    // 7. 更新任务为完成态
    let dest_str = dest_path.to_string_lossy().to_string();
    if let Err(e) = DownloadRepo::complete_task(db, &task_id, &dest_str) {
      tracing::warn!("完成任务状态写入失败 (task: {task_id}): {e}");
    }
    if tag_warning {
      tracing::warn!(
        "音频标签内嵌存在警告，但音频文件已成功下载保存至 {}",
        dest_str
      );
    }

    Ok(dest_path)
  }

  /// 为本地音频文件写入标签信息
  fn write_metadata_to_file(
    path: &Path,
    track: &Track,
    cover_data: &[u8],
  ) -> Result<(), Box<dyn std::error::Error>> {
    let mut tagged_file = Probe::open(path)?.read()?;
    let tag_type = tagged_file.primary_tag_type();
    let tag = match tagged_file.tag_mut(tag_type) {
      Some(t) => t,
      None => {
        tagged_file.insert_tag(Tag::new(tag_type));
        match tagged_file.tag_mut(tag_type) {
          Some(t) => t,
          None => return Err("无法初始化音频标签".into()),
        }
      }
    };

    tag.set_title(track.title.clone());
    tag.set_artist(track.artists_text());
    if let Some(ref album) = track.album {
      tag.set_album(album.name.clone());
    }

    if !cover_data.is_empty() {
      let pic = Picture::unchecked(cover_data.to_vec())
        .pic_type(PictureType::CoverFront)
        .build();
      tag.push_picture(pic);
    }

    tag.save_to_path(path, lofty::config::WriteOptions::default())?;
    Ok(())
  }

  /// 下载领域动作统一调度入口
  pub fn dispatch(
    db: &Database,
    client: &NcmClient,
    quality: &str,
    event_tx: &UnboundedSender<AppEvent>,
    state: Arc<RwLock<AppState>>,
    action: UiAction,
  ) {
    match action {
      UiAction::DownloadTrack(id) => {
        Self::download_track_flow(db, client, quality, event_tx, state, id);
      }
      UiAction::OpenDownloadFolder => {
        Self::open_download_folder();
      }
      _ => {}
    }
  }

  /// 触发离线下载任务编排流程
  pub fn download_track_flow(
    db: &Database,
    client: &NcmClient,
    quality: &str,
    event_tx: &UnboundedSender<AppEvent>,
    state: Arc<RwLock<AppState>>,
    track_id: String,
  ) {
    let dl_dir = Self::default_download_dir();
    let db_c = db.clone();
    let client_c = client.clone();
    let quality_c = quality.to_string();
    let ev_c = event_tx.clone();

    tokio::spawn(async move {
      let target_track = {
        let st = state.read();
        st.queue
          .iter()
          .find(|t| t.id == track_id)
          .cloned()
          .or_else(|| st.liked_songs.iter().find(|t| t.id == track_id).cloned())
          .or_else(|| st.daily_songs.iter().find(|t| t.id == track_id).cloned())
          .or_else(|| {
            st.artist_hot_songs
              .iter()
              .find(|t| t.id == track_id)
              .cloned()
          })
          .or_else(|| {
            st.current_track
              .as_ref()
              .filter(|t| t.id == track_id)
              .cloned()
          })
      };

      let track_opt = if let Some(t) = target_track {
        Some(t)
      } else {
        match client_c.song().get_song_detail(&[&track_id]).await {
          Ok(mut details) if !details.is_empty() => Some(details.remove(0)),
          _ => None,
        }
      };

      if let Some(track) = track_opt {
        let tid = track.id.clone();
        let title = track.title.clone();
        match Self::download_track(&db_c, track, &quality_c, &dl_dir, &client_c).await {
          Ok(saved) => {
            tracing::info!("曲目下载成功: {title} ({:?})", saved);
            let _ = ev_c.send(AppEvent::ShowToast(format!("下载完成: {title}")));
          }
          Err(err) => {
            tracing::error!("曲目下载失败 (track: {tid}): {err}");
            let _ = ev_c.send(AppEvent::ShowToast(format!("下载失败: {title}")));
          }
        }
        let db_query = db_c.clone();
        if let Ok(Ok(tasks)) =
          tokio::task::spawn_blocking(move || Self::list_tasks(&db_query)).await
        {
          let _ = ev_c.send(AppEvent::DownloadsLoaded(tasks));
        }
      } else {
        tracing::warn!("未能找到待下载曲目元数据 (track_id: {track_id})");
        let _ = ev_c.send(AppEvent::ShowToast("未找到歌曲详情，下载失败".to_string()));
      }
    });
  }

  /// 打开本地离线下载文件夹
  pub fn open_download_folder() {
    let dl_dir = Self::default_download_dir();
    let _ = std::fs::create_dir_all(&dl_dir);
    #[cfg(target_os = "linux")]
    {
      let _ = std::process::Command::new("xdg-open").arg(&dl_dir).spawn();
    }
    #[cfg(target_os = "windows")]
    {
      let _ = std::process::Command::new("explorer").arg(&dl_dir).spawn();
    }
    #[cfg(target_os = "macos")]
    {
      let _ = std::process::Command::new("open").arg(&dl_dir).spawn();
    }
  }
}
