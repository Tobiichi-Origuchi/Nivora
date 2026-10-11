use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

use crate::api::NcmClient;
use crate::common::channel::{AppEvent, UiAction};
use crate::common::state::AppState;
use crate::common::types::NavPage;
use crate::data::repo_settings::SettingsRepo;
use crate::service::context::AppContext;
use crate::service::cover_svc::CoverService;

/// 页面路由导航与界面状态管理服务
pub struct NavService;

impl NavService {
  /// 导航路由、窗口弹窗与界面状态动作统一调度入口
  pub fn dispatch(ctx: &AppContext, action: UiAction) {
    match action {
      UiAction::NavigateTo(page) => {
        Self::navigate_to(&ctx.state, &ctx.event_tx, page);
      }
      UiAction::GoBack => Self::go_back(&ctx.state, &ctx.event_tx),
      UiAction::GoForward => Self::go_forward(&ctx.state, &ctx.event_tx),
      UiAction::NavigateToArtist { id, name } => {
        Self::navigate_to_artist(
          &ctx.client,
          ctx.state.clone(),
          ctx.event_tx.clone(),
          id,
          name,
        );
      }
      UiAction::Search(kw) => {
        Self::search(&ctx.client, ctx.state.clone(), ctx.event_tx.clone(), kw);
      }
      UiAction::ToggleLyricView => {
        let val = {
          let mut s = ctx.state.write();
          s.is_lyric_expanded = !s.is_lyric_expanded;
          s.is_lyric_expanded
        };
        let _ = ctx.event_tx.send(AppEvent::LyricViewToggled(val));
      }
      UiAction::ToggleQueueDrawer => {
        let val = {
          let mut s = ctx.state.write();
          s.is_queue_open = !s.is_queue_open;
          s.is_queue_open
        };
        let _ = ctx.event_tx.send(AppEvent::QueueDrawerToggled(val));
      }
      UiAction::ToggleSidebar => {
        let val = {
          let mut s = ctx.state.write();
          s.is_sidebar_collapsed = !s.is_sidebar_collapsed;
          s.is_sidebar_collapsed
        };
        if let Err(e) = SettingsRepo::set(&ctx.db, "sidebar_collapsed", &val.to_string()) {
          tracing::warn!("持久化侧边栏折叠状态失败: {e}");
        }
        let _ = ctx.event_tx.send(AppEvent::SidebarToggled(val));
      }
      UiAction::OpenEqualizer | UiAction::CloseEqualizer => {
        let open = matches!(action, UiAction::OpenEqualizer);
        ctx.state.write().is_equalizer_open = open;
        let _ = ctx.event_tx.send(AppEvent::EqualizerToggled(open));
      }
      UiAction::OpenSettings | UiAction::CloseSettings => {
        let open = matches!(action, UiAction::OpenSettings);
        ctx.state.write().is_settings_open = open;
        let _ = ctx.event_tx.send(AppEvent::SettingsToggled(open));
      }
      UiAction::OpenComments => {
        if let Some(track_id) = ctx
          .state
          .read()
          .current_track
          .as_ref()
          .map(|t| t.id.clone())
        {
          let _ = ctx.event_tx.send(AppEvent::CommentsModalToggled(true));
          let (client, ev_tx) = (ctx.client.clone(), ctx.event_tx.clone());
          tokio::spawn(async move {
            if let Ok(comments) = client.comment().get_music_comments(&track_id, 0, 30).await {
              let _ = ev_tx.send(AppEvent::CommentsLoaded(comments));
            }
          });
        }
      }
      UiAction::CloseComments => {
        let _ = ctx.event_tx.send(AppEvent::CommentsModalToggled(false));
      }
      _ => {}
    }
  }

  /// 触发页面导航并维护历史栈
  pub fn navigate_to(
    state: &RwLock<AppState>,
    event_tx: &UnboundedSender<AppEvent>,
    page: NavPage,
  ) {
    let current = state.read().current_page;
    if current != page {
      let mut s = state.write();
      s.nav_history.push(current);
      s.current_page = page;
      s.forward_history.clear();
      let can_back = !s.nav_history.is_empty();
      drop(s);
      let _ = event_tx.send(AppEvent::NavPageChanged(page));
      let _ = event_tx.send(AppEvent::CanGoBackChanged(can_back));
      let _ = event_tx.send(AppEvent::CanGoForwardChanged(false));
    }
  }

  /// 后退历史页面
  pub fn go_back(state: &RwLock<AppState>, event_tx: &UnboundedSender<AppEvent>) {
    let mut s = state.write();
    if let Some(prev) = s.nav_history.pop() {
      let cur = s.current_page;
      s.forward_history.push(cur);
      s.current_page = prev;
      let (b, f) = (!s.nav_history.is_empty(), !s.forward_history.is_empty());
      drop(s);
      let _ = event_tx.send(AppEvent::NavPageChanged(prev));
      let _ = event_tx.send(AppEvent::CanGoBackChanged(b));
      let _ = event_tx.send(AppEvent::CanGoForwardChanged(f));
    }
  }

  /// 前进历史页面
  pub fn go_forward(state: &RwLock<AppState>, event_tx: &UnboundedSender<AppEvent>) {
    let mut s = state.write();
    if let Some(next) = s.forward_history.pop() {
      let cur = s.current_page;
      s.nav_history.push(cur);
      s.current_page = next;
      let (b, f) = (!s.nav_history.is_empty(), !s.forward_history.is_empty());
      drop(s);
      let _ = event_tx.send(AppEvent::NavPageChanged(next));
      let _ = event_tx.send(AppEvent::CanGoBackChanged(b));
      let _ = event_tx.send(AppEvent::CanGoForwardChanged(f));
    }
  }

  /// 导航至歌手主页并拉取热门单曲与封面
  pub fn navigate_to_artist(
    client: &NcmClient,
    state: Arc<RwLock<AppState>>,
    event_tx: UnboundedSender<AppEvent>,
    id: String,
    name: String,
  ) {
    info!("导航至歌手主页: {name} ({id})");
    Self::navigate_to(&state, &event_tx, NavPage::Artist);
    state.write().current_artist_id = Some(id.clone());
    let (client, ev_tx, st) = (client.clone(), event_tx.clone(), state.clone());
    tokio::spawn(async move {
      if let Ok((artist, desc, hot_songs)) = client.artist().get_artist_detail(&id).await
        && st.read().current_artist_id.as_deref() == Some(&id)
      {
        st.write().artist_hot_songs = hot_songs.clone();
        let _ = ev_tx.send(AppEvent::ArtistLoaded {
          artist: artist.clone(),
          desc: desc.clone(),
          tracks: hot_songs.clone(),
        });
        let count = hot_songs.len();
        CoverService::spawn_artist_covers_loader(artist, desc, hot_songs, count, ev_tx, st);
      }
    });
  }

  /// 执行全局搜索并拉取封面
  pub fn search(
    client: &NcmClient,
    state: Arc<RwLock<AppState>>,
    event_tx: UnboundedSender<AppEvent>,
    kw: String,
  ) {
    info!("执行全局搜索: {kw}");
    Self::navigate_to(&state, &event_tx, NavPage::Search);
    let (client, ev_tx) = (client.clone(), event_tx.clone());
    tokio::spawn(async move {
      let songs = client
        .search()
        .search_songs(&kw, 30, 0)
        .await
        .unwrap_or_default();
      let playlists = client
        .search()
        .search_playlists(&kw, 12, 0)
        .await
        .unwrap_or_default();
      let _ = ev_tx.send(AppEvent::SearchResultsLoaded {
        songs: songs.clone(),
        playlists: playlists.clone(),
      });
      CoverService::spawn_search_covers_loader(songs, playlists, ev_tx);
    });
  }
}
