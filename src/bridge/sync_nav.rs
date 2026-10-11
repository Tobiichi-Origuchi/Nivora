use slint::{ComponentHandle, Model, VecModel};
use std::rc::Rc;

use crate::bridge::MainWindow;
use crate::bridge::sync_player::format_time_ms;
use crate::bridge::{
  CommentUi, DownloadTaskUi, NavState, PageDataState, PlayerState, PlaylistUi, TrackUi,
};
use crate::common::model::{Artist, Comment, DownloadTask, Playlist, Track};
use crate::common::types::NavPage;

/// 页面路由导航与数据页面属性同步器
pub struct SyncNav;

impl SyncNav {
  /// 同步当前页面路由
  pub fn sync_page(ui: &MainWindow, page: NavPage) {
    let page_str = match page {
      NavPage::Home => "home",
      NavPage::Daily => "daily",
      NavPage::Liked => "liked",
      NavPage::Playlist => "playlist",
      NavPage::Library => "library",
      NavPage::Search => "search",
      NavPage::Artist => "artist",
      NavPage::Stats => "stats",
    };
    ui.global::<NavState>().set_current_page(page_str.into());
  }

  /// 同步返回历史按钮可用状态
  pub fn sync_can_go_back(ui: &MainWindow, can_go_back: bool) {
    ui.global::<NavState>().set_can_go_back(can_go_back);
  }

  /// 同步前进历史按钮可用状态
  pub fn sync_can_go_forward(ui: &MainWindow, can_go_forward: bool) {
    ui.global::<NavState>().set_can_go_forward(can_go_forward);
  }

  /// 同步歌单列表
  pub fn sync_playlists(ui: &MainWindow, playlists: &[Playlist]) {
    let page_global = ui.global::<PageDataState>();
    let ui_playlists: Vec<PlaylistUi> = playlists
      .iter()
      .map(|p| {
        let cover_url = p.cover_url.clone().unwrap_or_default();
        let (cover_image, has_cover) =
          crate::service::CoverService::try_get_cached_image(&cover_url);
        PlaylistUi {
          id: p.id.clone().into(),
          title: p.title.clone().into(),
          cover_url: cover_url.into(),
          cover_image,
          has_cover,
          track_count: p.track_count as i32,
          play_count_str: format_play_count(p.play_count).into(),
          creator_name: p.creator_name.clone().unwrap_or_default().into(),
          description: p.description.clone().unwrap_or_default().into(),
        }
      })
      .collect();

    let half = ui_playlists.len().div_ceil(2);
    let row1: Vec<PlaylistUi> = ui_playlists.iter().take(half).cloned().collect();
    let row2: Vec<PlaylistUi> = ui_playlists.iter().skip(half).cloned().collect();
    page_global.set_home_recommend_row1(Rc::new(VecModel::from(row1)).into());
    page_global.set_home_recommend_row2(Rc::new(VecModel::from(row2)).into());

    let model = Rc::new(VecModel::from(ui_playlists));
    page_global.set_home_recommend_playlists(model.clone().into());
    page_global.set_user_playlists(model.into());
  }

  /// 同步每日推荐歌曲
  pub fn sync_daily_songs(
    ui: &MainWindow,
    tracks: &[Track],
    liked_ids: &std::collections::HashSet<String>,
  ) {
    let page_global = ui.global::<PageDataState>();
    let ui_tracks = Self::to_track_ui_vec(tracks, Some(liked_ids));
    let model = Rc::new(VecModel::from(ui_tracks));
    page_global.set_daily_songs(model.into());
  }

  /// 同步我喜欢的歌曲
  pub fn sync_liked_songs(ui: &MainWindow, tracks: &[Track]) {
    let page_global = ui.global::<PageDataState>();
    let ui_tracks = Self::to_track_ui_vec(tracks, None);
    let model = Rc::new(VecModel::from(ui_tracks));
    page_global.set_liked_songs(model.into());
  }

  /// 同步本地音乐库歌曲
  pub fn sync_local_tracks(
    ui: &MainWindow,
    tracks: &[Track],
    liked_ids: &std::collections::HashSet<String>,
  ) {
    let page_global = ui.global::<PageDataState>();
    let count = tracks.len();
    let ui_tracks = Self::to_track_ui_vec(tracks, Some(liked_ids));
    let model = Rc::new(VecModel::from(ui_tracks));
    page_global.set_library_tracks(model.into());
    page_global.set_library_scan_progress(format!("共收录 {} 首本地音频", count).into());
  }

  /// 同步搜索中心结果
  pub fn sync_search_results(
    ui: &MainWindow,
    songs: &[Track],
    playlists: &[Playlist],
    liked_ids: &std::collections::HashSet<String>,
  ) {
    let page_global = ui.global::<PageDataState>();

    let ui_tracks = Self::to_track_ui_vec(songs, Some(liked_ids));
    page_global.set_search_songs(Rc::new(VecModel::from(ui_tracks)).into());

    let ui_playlists: Vec<PlaylistUi> = playlists
      .iter()
      .map(|p| {
        let cover_url = p.cover_url.clone().unwrap_or_default();
        let (cover_image, has_cover) =
          crate::service::CoverService::try_get_cached_image(&cover_url);
        PlaylistUi {
          id: p.id.clone().into(),
          title: p.title.clone().into(),
          cover_url: cover_url.into(),
          cover_image,
          has_cover,
          track_count: p.track_count as i32,
          play_count_str: format_play_count(p.play_count).into(),
          creator_name: p.creator_name.clone().unwrap_or_default().into(),
          description: p.description.clone().unwrap_or_default().into(),
        }
      })
      .collect();

    let half = ui_playlists.len().div_ceil(2);
    let row1: Vec<PlaylistUi> = ui_playlists.iter().take(half).cloned().collect();
    let row2: Vec<PlaylistUi> = ui_playlists.iter().skip(half).cloned().collect();
    page_global.set_search_playlists_row1(Rc::new(VecModel::from(row1)).into());
    page_global.set_search_playlists_row2(Rc::new(VecModel::from(row2)).into());
    page_global.set_search_playlists(Rc::new(VecModel::from(ui_playlists)).into());
  }

  /// 同步歌单封面图片
  pub fn sync_playlist_cover(ui: &MainWindow, img: slint::Image) {
    let page_global = ui.global::<PageDataState>();
    page_global.set_current_playlist_cover_image(img);
    page_global.set_has_playlist_cover(true);
  }

  /// 同步动态系统真实日期
  pub fn sync_system_date(ui: &MainWindow, day: &str, month: &str, weekday: &str) {
    let page_global = ui.global::<PageDataState>();
    page_global.set_daily_date_day(day.into());
    page_global.set_daily_date_month(month.into());
    page_global.set_daily_date_weekday(weekday.into());
  }

  /// 同步热门搜索词
  pub fn sync_hot_keywords(ui: &MainWindow, keywords: &[String]) {
    let page_global = ui.global::<PageDataState>();
    let slint_strings: Vec<slint::SharedString> =
      keywords.iter().map(|s| s.as_str().into()).collect();
    page_global.set_hot_keywords(Rc::new(VecModel::from(slint_strings)).into());
  }

  /// 同步歌单详情
  pub fn sync_playlist_detail(
    ui: &MainWindow,
    playlist: &Playlist,
    tracks: &[Track],
    liked_ids: &std::collections::HashSet<String>,
  ) {
    let page_global = ui.global::<PageDataState>();

    let cover_url = playlist.cover_url.clone().unwrap_or_default();
    let (cover_image, has_cover) = crate::service::CoverService::try_get_cached_image(&cover_url);
    if has_cover || !page_global.get_has_playlist_cover() {
      page_global.set_current_playlist_cover_image(cover_image.clone());
      page_global.set_has_playlist_cover(has_cover);
    }

    let p_ui = PlaylistUi {
      id: playlist.id.clone().into(),
      title: playlist.title.clone().into(),
      cover_url: cover_url.into(),
      cover_image: if has_cover {
        cover_image
      } else {
        page_global.get_current_playlist_cover_image()
      },
      has_cover: has_cover || page_global.get_has_playlist_cover(),
      track_count: playlist.track_count as i32,
      play_count_str: format_play_count(playlist.play_count).into(),
      creator_name: playlist.creator_name.clone().unwrap_or_default().into(),
      description: playlist.description.clone().unwrap_or_default().into(),
    };
    page_global.set_current_playlist(p_ui);

    let ui_tracks = Self::to_track_ui_vec(tracks, Some(liked_ids));
    page_global.set_playlist_tracks(Rc::new(VecModel::from(ui_tracks)).into());
  }

  /// 同步首页推荐新歌
  pub fn sync_home_recommend_songs(
    ui: &MainWindow,
    songs: &[Track],
    liked_ids: &std::collections::HashSet<String>,
  ) {
    let page_global = ui.global::<PageDataState>();
    let ui_tracks = Self::to_track_ui_vec(songs, Some(liked_ids));
    page_global.set_home_recommend_songs(Rc::new(VecModel::from(ui_tracks)).into());
  }

  /// 同步继续聆听
  pub fn sync_continue_listening(
    ui: &MainWindow,
    tracks: &[Track],
    liked_ids: &std::collections::HashSet<String>,
  ) {
    let page_global = ui.global::<PageDataState>();
    let ui_tracks = Self::to_track_ui_vec(tracks, Some(liked_ids));
    page_global.set_continue_listening_tracks(Rc::new(VecModel::from(ui_tracks)).into());
  }

  /// 同步歌曲评论列表
  pub fn sync_comments(ui: &MainWindow, comments: &[Comment]) {
    let page_global = ui.global::<PageDataState>();
    let ui_comments: Vec<CommentUi> = comments
      .iter()
      .map(|c| CommentUi {
        user_nickname: c.user_nickname.clone().into(),
        user_avatar: c.user_avatar.clone().into(),
        content: c.content.clone().into(),
        time_str: c.time_str.clone().into(),
        liked_count: c.liked_count,
      })
      .collect();
    page_global.set_current_comments(Rc::new(VecModel::from(ui_comments)).into());
  }

  /// 同步歌手详情与作品
  pub fn sync_artist_detail(
    ui: &MainWindow,
    artist: &Artist,
    desc: &str,
    tracks: &[Track],
    liked_ids: &std::collections::HashSet<String>,
  ) {
    let page_global = ui.global::<PageDataState>();
    page_global.set_artist_name(artist.name.clone().into());
    page_global.set_artist_avatar(artist.avatar.clone().unwrap_or_default().into());
    page_global.set_artist_desc(desc.into());
    let ui_tracks = Self::to_track_ui_vec(tracks, Some(liked_ids));
    page_global.set_artist_hot_songs(Rc::new(VecModel::from(ui_tracks)).into());
  }

  /// 同步下载任务列表
  pub fn sync_downloads(ui: &MainWindow, tasks: &[DownloadTask]) {
    let page_global = ui.global::<PageDataState>();
    let ui_tasks: Vec<DownloadTaskUi> = tasks
      .iter()
      .map(|t| {
        let progress = if t.total > 0 {
          (t.received as f32 / t.total as f32).clamp(0.0, 1.0)
        } else if t.status == "completed" {
          1.0
        } else {
          0.0
        };
        let size_str = format!("{}/{}", format_size(t.received), format_size(t.total));
        DownloadTaskUi {
          task_id: t.task_id.clone().into(),
          title: t.track.title.clone().into(),
          artist: t.track.artists_text().into(),
          quality: t.quality_level.clone().into(),
          status: t.status.clone().into(),
          progress,
          size_str: size_str.into(),
        }
      })
      .collect();
    page_global.set_download_tasks(Rc::new(VecModel::from(ui_tasks)).into());
  }

  /// 同步库管理 Tab
  pub fn sync_library_tab(ui: &MainWindow, tab: &str) {
    ui.global::<PageDataState>().set_library_tab(tab.into());
  }

  /// 同步评论弹窗显隐
  pub fn sync_comments_modal(ui: &MainWindow, is_open: bool) {
    ui.global::<PlayerState>().set_is_comment_open(is_open);
  }

  /// 同步用户听歌行为统计与动态问候语
  pub fn sync_listening_stats(
    ui: &MainWindow,
    greeting: &str,
    stats: &crate::common::model::UserListeningStats,
  ) {
    let page_global = ui.global::<PageDataState>();
    page_global.set_greeting_text(greeting.into());
    page_global.set_weekly_listen_hours(stats.weekly_hours_str.as_str().into());
    page_global.set_total_listen_count(stats.total_tracks_played as i32);
    page_global.set_streak_days(stats.streak_days as i32);
    page_global.set_total_favorites(stats.total_favorites as i32);
    page_global.set_weekly_growth_str(stats.weekly_growth_str.as_str().into());
    page_global.set_favorite_ratio_str(stats.favorite_ratio_str.as_str().into());
  }

  /// 更新当前已加载所有视图中的红心收藏状态
  pub fn sync_track_like_toggled(ui: &MainWindow, track_id: &str, is_liked: bool) {
    let page_global = ui.global::<PageDataState>();
    Self::update_model_liked(page_global.get_daily_songs(), track_id, is_liked, |m| {
      page_global.set_daily_songs(m);
    });
    Self::update_model_liked(page_global.get_playlist_tracks(), track_id, is_liked, |m| {
      page_global.set_playlist_tracks(m);
    });
    Self::update_model_liked(page_global.get_search_songs(), track_id, is_liked, |m| {
      page_global.set_search_songs(m);
    });
    Self::update_model_liked(
      page_global.get_home_recommend_songs(),
      track_id,
      is_liked,
      |m| {
        page_global.set_home_recommend_songs(m);
      },
    );
    Self::update_model_liked(
      page_global.get_continue_listening_tracks(),
      track_id,
      is_liked,
      |m| {
        page_global.set_continue_listening_tracks(m);
      },
    );
    Self::update_model_liked(
      page_global.get_artist_hot_songs(),
      track_id,
      is_liked,
      |m| {
        page_global.set_artist_hot_songs(m);
      },
    );
    Self::update_model_liked(page_global.get_library_tracks(), track_id, is_liked, |m| {
      page_global.set_library_tracks(m);
    });
  }

  fn update_model_liked<F>(model: slint::ModelRc<TrackUi>, track_id: &str, is_liked: bool, apply: F)
  where
    F: FnOnce(slint::ModelRc<TrackUi>),
  {
    let count = model.row_count();
    if count == 0 {
      return;
    }
    let mut changed = false;
    let mut items: Vec<TrackUi> = (0..count).filter_map(|i| model.row_data(i)).collect();
    for item in &mut items {
      if item.id.as_str() == track_id {
        item.is_liked = is_liked;
        changed = true;
      }
    }
    if changed {
      apply(Rc::new(VecModel::from(items)).into());
    }
  }

  fn to_track_ui_vec(
    tracks: &[Track],
    liked_ids: Option<&std::collections::HashSet<String>>,
  ) -> Vec<TrackUi> {
    tracks
      .iter()
      .enumerate()
      .map(|(idx, t)| {
        let is_liked = match liked_ids {
          Some(ids) => ids.contains(&t.id),
          None => true,
        };
        let cover_url = t
          .cover_url
          .clone()
          .or_else(|| t.album.as_ref().and_then(|a| a.cover.clone()))
          .unwrap_or_default();
        let (cover_image, has_cover) =
          crate::service::CoverService::try_get_cached_image_limited(&cover_url, idx < 15);
        TrackUi {
          id: t.id.clone().into(),
          artist_id: t
            .artists
            .first()
            .map(|a| a.id.clone())
            .unwrap_or_default()
            .into(),
          title: t.title.clone().into(),
          artists: t.artists_text().into(),
          album: t
            .album
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_default()
            .into(),
          duration_str: format_time_ms(t.duration_ms).into(),
          duration_ms: t.duration_ms as i32,
          cover_url: cover_url.into(),
          cover_image,
          has_cover,
          is_vip: t.fee == 1,
          is_liked,
        }
      })
      .collect()
  }
}

fn format_play_count(count: u64) -> String {
  if count >= 100_000_000 {
    format!("{:.1}亿", count as f64 / 100_000_000.0)
  } else if count >= 10_000 {
    format!("{:.0}万", count as f64 / 10_000.0)
  } else {
    count.to_string()
  }
}

fn format_size(bytes: u64) -> String {
  if bytes >= 1024 * 1024 {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
  } else if bytes >= 1024 {
    format!("{:.1} KB", bytes as f64 / 1024.0)
  } else {
    format!("{} B", bytes)
  }
}
