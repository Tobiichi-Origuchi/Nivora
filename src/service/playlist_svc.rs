use chrono::Timelike;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

use crate::api::NcmClient;
use crate::common::channel::{AppEvent, UiAction};
use crate::common::model::Track;
use crate::common::state::AppState;
use crate::common::types::NavPage;
use crate::data::Database;
use crate::data::repo_history::HistoryRepo;
use crate::data::repo_playlist::PlaylistRepo;
use crate::service::cover_svc::CoverService;
use crate::service::nav_svc::NavService;

/// 歌单、收藏与足迹管理领域服务
pub struct PlaylistService;

impl PlaylistService {
  /// 根据系统当前时刻返回问候语
  pub fn get_time_greeting() -> &'static str {
    let hour = chrono::Local::now().hour();
    match hour {
      5..=11 => "早上好",
      12..=13 => "中午好",
      14..=17 => "下午好",
      18..=22 => "晚上好",
      _ => "夜深了",
    }
  }

  /// 重新查询真实听歌足迹并推送 ListeningStatsLoaded 事件
  pub fn refresh_stats(db: &Database, ev_tx: &mpsc::UnboundedSender<AppEvent>) {
    let db_c = db.clone();
    let ev_c = ev_tx.clone();
    tokio::task::spawn_blocking(move || {
      if let Ok(stats) = HistoryRepo::get_listening_stats(&db_c) {
        let greeting = Self::get_time_greeting().to_string();
        let _ = ev_c.send(AppEvent::ListeningStatsLoaded { greeting, stats });
      }
    });
  }

  /// 歌单领域动作统一调度入口
  pub fn dispatch(
    client: &NcmClient,
    db: &Database,
    state: Arc<RwLock<AppState>>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    action: UiAction,
  ) {
    match action {
      UiAction::OpenPlaylistDetail(id) => {
        Self::open_playlist_detail(client, state, event_tx, id);
      }
      UiAction::ToggleTrackLike(id) => {
        Self::toggle_track_like(client, db, state, event_tx, id);
      }
      UiAction::CollectPlaylist(id) => {
        Self::collect_playlist(client, db, state, event_tx, id);
      }
      UiAction::CreateNewPlaylist => {
        Self::create_new_playlist(db, event_tx);
      }
      _ => {}
    }
  }

  /// 打开并加载歌单详情与曲目
  pub fn open_playlist_detail(
    client: &NcmClient,
    state: Arc<RwLock<AppState>>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    playlist_id: String,
  ) {
    info!("打开歌单详情: {playlist_id}");
    // 1. 路由跳转至歌单详情页并维护历史栈
    NavService::navigate_to(&state, &event_tx, NavPage::Playlist);

    // 2. 标记当前活跃歌单 ID，用于防并发串台
    state.write().current_playlist_id = Some(playlist_id.clone());

    let client = client.clone();
    let ev_tx = event_tx.clone();
    let st = state.clone();

    tokio::spawn(async move {
      match client.playlist().get_playlist_detail(&playlist_id).await {
        Ok((playlist, mut tracks)) => {
          if tracks.is_empty()
            && let Ok(more) = client.playlist().get_playlist_tracks(&playlist_id, 0).await
          {
            tracks = more;
          }
          if st.read().current_playlist_id.as_deref() != Some(&playlist_id) {
            return;
          }
          if let Some(cover) = playlist.cover_url.clone() {
            let ev_tx_c = ev_tx.clone();
            let key = format!("pl_{}", playlist.id);
            tokio::spawn(async move {
              if let Some((path, _)) = CoverService::load_cover(&cover, &key).await {
                let _ = ev_tx_c.send(AppEvent::PlaylistCoverLoaded(path));
              }
            });
          }
          let _ = ev_tx.send(AppEvent::PlaylistDetailLoaded {
            playlist: playlist.clone(),
            tracks: tracks.clone(),
          });
          let cover_count = tracks.len();
          CoverService::spawn_playlist_detail_covers_loader(
            playlist,
            tracks,
            cover_count,
            ev_tx,
            st,
          );
        }
        Err(e) => {
          tracing::error!("加载歌单详情失败 (id: {playlist_id}): {e}");
          let _ = ev_tx.send(AppEvent::ShowToast(
            "加载歌单失败，请检查网络连接".to_string(),
          ));
        }
      }
    });
  }

  /// 切换歌曲红心收藏
  pub fn toggle_track_like(
    client: &NcmClient,
    db: &Database,
    state: Arc<RwLock<AppState>>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    track_id: String,
  ) {
    let client = client.clone();
    let db = db.clone();
    let ev_tx = event_tx.clone();
    let st = state.clone();

    let (target_track, is_currently_liked) = {
      let s = st.read();
      let is_liked = s.liked_songs.iter().any(|t| t.id == track_id);
      let track = s
        .queue
        .iter()
        .find(|t| t.id == track_id)
        .cloned()
        .or_else(|| s.liked_songs.iter().find(|t| t.id == track_id).cloned())
        .or_else(|| s.daily_songs.iter().find(|t| t.id == track_id).cloned())
        .or_else(|| {
          s.artist_hot_songs
            .iter()
            .find(|t| t.id == track_id)
            .cloned()
        })
        .or_else(|| {
          s.current_track
            .as_ref()
            .filter(|t| t.id == track_id)
            .cloned()
        });
      (track, is_liked)
    };

    let new_like = !is_currently_liked;

    // 1. 立即更新内存中的 liked_songs 快照并向 UI 广播实时状态变更 (零感知延迟)
    {
      let mut s = st.write();
      if new_like {
        if let Some(ref t) = target_track
          && !s.liked_songs.iter().any(|x| x.id == track_id)
        {
          s.liked_songs.push(t.clone());
        }
      } else {
        s.liked_songs.retain(|x| x.id != track_id);
      }
    }
    let _ = ev_tx.send(AppEvent::TrackLikeToggled {
      track_id: track_id.clone(),
      is_liked: new_like,
    });

    tokio::spawn(async move {
      if let Err(err) = client.song().like_song(&track_id, new_like).await {
        tracing::warn!("网络红心收藏请求同步失败 (track_id: {track_id}): {err}");
      }
      let action = if new_like { "like" } else { "unlike" };
      let track_opt = if let Some(t) = target_track {
        Some(t)
      } else {
        match client.song().get_song_detail(&[&track_id]).await {
          Ok(mut details) if !details.is_empty() => Some(details.remove(0)),
          _ => None,
        }
      };

      if let Some(track) = track_opt {
        let db_c = db.clone();
        let tr_c = track.clone();
        let act = action.to_string();
        let tid = track.id.clone();
        let res =
          tokio::task::spawn_blocking(move || HistoryRepo::record_favorite(&db_c, &tr_c, &act))
            .await;
        if let Ok(Err(err)) = res {
          tracing::error!("本地收藏记录失败 (track_id: {tid}): {err}");
        }
      }

      let db_c = db.clone();
      if let Ok(Ok(favs)) =
        tokio::task::spawn_blocking(move || HistoryRepo::list_favorites(&db_c, 1000)).await
      {
        let liked: Vec<Track> = favs.into_iter().map(|f| f.track).collect();
        st.write().liked_songs = liked.clone();
        let _ = ev_tx.send(AppEvent::LikedSongsLoaded(liked.clone()));
        let count = liked.len();
        CoverService::spawn_tracks_covers_loader(
          liked,
          count,
          ev_tx.clone(),
          AppEvent::LikedSongsLoaded,
        );
      }

      if st.read().current_track.as_ref().map(|t| &t.id) == Some(&track_id) {
        let _ = ev_tx.send(AppEvent::TrackLikeStateChanged(new_like));
      }

      Self::refresh_stats(&db, &ev_tx);
    });
  }

  /// 收藏当前/每日/在线歌单
  pub fn collect_playlist(
    client: &NcmClient,
    db: &Database,
    state: Arc<RwLock<AppState>>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    pl_id: String,
  ) {
    info!("执行歌单/曲目收藏操作: {pl_id}");
    let db = db.clone();
    let st = state.clone();
    let ev_tx = event_tx.clone();
    let client = client.clone();
    tokio::spawn(async move {
      if pl_id == "daily" {
        let daily = { st.read().daily_songs.clone() };
        let db_c = db.clone();
        let _res = tokio::task::spawn_blocking(move || {
          for track in &daily {
            if let Err(err) = HistoryRepo::record_favorite(&db_c, track, "like") {
              tracing::warn!("批量收藏每日推荐曲目失败 (track: {}): {err}", track.id);
            }
          }
        })
        .await;
      } else if pl_id == "current" {
        let cur_track = { st.read().current_track.clone() };
        if let Some(track) = cur_track {
          let db_c = db.clone();
          let tid = track.id.clone();
          let res = tokio::task::spawn_blocking(move || {
            HistoryRepo::record_favorite(&db_c, &track, "like")
          })
          .await;
          if let Ok(Err(err)) = res {
            tracing::error!("收藏当前曲目失败 (track: {tid}): {err}");
          }
        }
      } else {
        match client.playlist().get_playlist_tracks(&pl_id, 0).await {
          Ok(tracks) => {
            let db_c = db.clone();
            let _res = tokio::task::spawn_blocking(move || {
              for track in &tracks {
                if let Err(err) = HistoryRepo::record_favorite(&db_c, track, "like") {
                  tracing::warn!("批量收藏歌单曲目失败 (track: {}): {err}", track.id);
                }
              }
            })
            .await;
          }
          Err(err) => {
            tracing::warn!("拉取歌单曲目失败 (pl_id: {pl_id}): {err}");
            let _ = ev_tx.send(AppEvent::ShowToast("获取待收藏歌单曲目失败".to_string()));
          }
        }
      }

      let db_c = db.clone();
      if let Ok(Ok(favs)) =
        tokio::task::spawn_blocking(move || HistoryRepo::list_favorites(&db_c, 1000)).await
      {
        let liked: Vec<Track> = favs.into_iter().map(|f| f.track).collect();
        st.write().liked_songs = liked.clone();
        let _ = ev_tx.send(AppEvent::LikedSongsLoaded(liked.clone()));
        let count = liked.len();
        CoverService::spawn_tracks_covers_loader(
          liked,
          count,
          ev_tx.clone(),
          AppEvent::LikedSongsLoaded,
        );
      }

      Self::refresh_stats(&db, &ev_tx);
    });
  }

  /// 创建新的本地自定义歌单
  pub fn create_new_playlist(db: &Database, event_tx: mpsc::UnboundedSender<AppEvent>) {
    info!("新建本地歌单");
    let db = db.clone();
    let ev_tx = event_tx.clone();
    tokio::spawn(async move {
      let db_c = db.clone();
      let count = tokio::task::spawn_blocking(move || {
        PlaylistRepo::list_playlists(&db_c)
          .map(|list| list.len())
          .unwrap_or(0)
      })
      .await
      .unwrap_or(0);

      let id = format!("local_pl_{}", rand::random::<u32>());
      let title = format!("新建歌单 {}", count + 1);
      let db_c2 = db.clone();
      let title_c = title.clone();
      let create_res = tokio::task::spawn_blocking(move || {
        PlaylistRepo::create_playlist(&db_c2, &id, &title_c, Some("用户本地创建的歌单"), None)
      })
      .await;

      match create_res {
        Ok(Ok(_)) => {
          let db_c3 = db.clone();
          if let Ok(Ok(playlists)) =
            tokio::task::spawn_blocking(move || PlaylistRepo::list_playlists(&db_c3)).await
          {
            let _ = ev_tx.send(AppEvent::PlaylistsLoaded(playlists));
            let _ = ev_tx.send(AppEvent::ShowToast(format!("已成功创建歌单: {title}")));
          }
        }
        Ok(Err(err)) => {
          tracing::error!("创建歌单数据库写入失败: {err}");
          let _ = ev_tx.send(AppEvent::ShowToast("创建歌单失败，数据库错误".to_string()));
        }
        Err(e) => {
          tracing::error!("创建歌单任务异常: {e}");
        }
      }
    });
  }
}
