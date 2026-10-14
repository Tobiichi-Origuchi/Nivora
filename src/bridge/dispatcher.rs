use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::mpsc;
use tracing::info;

use crate::bridge::channel::UiAction;
use crate::service::{
  AppContext, AuthService, DownloadService, LibraryService, LyricService, NavService,
  PlaybackService, PlaylistService, ThemeService,
};

/// 全局 UI 动作分发与路由调度器
#[derive(Clone)]
pub struct ActionDispatcher {
  pub ctx: AppContext,
  pub qr_polling_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}

impl ActionDispatcher {
  pub fn new(ctx: AppContext, qr_polling_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>) -> Self {
    Self {
      ctx,
      qr_polling_cancel,
    }
  }

  /// 启动 UiAction 监听与调度任务
  pub fn spawn(
    self,
    rt: &tokio::runtime::Runtime,
    mut action_rx: mpsc::UnboundedReceiver<UiAction>,
  ) {
    rt.spawn(async move {
      while let Some(action) = action_rx.recv().await {
        info!("收到 UI 动作指令: {:?}", action);
        self.handle_action(action).await;
      }
    });
  }

  /// 启动音频播放状态与频谱轮询任务
  pub fn spawn_playback_ticker(&self, rt: &tokio::runtime::Runtime) {
    PlaybackService::spawn_playback_ticker(&self.ctx, rt);
  }

  /// 单个 UI 动作的分发路由器
  pub async fn handle_action(&self, action: UiAction) {
    match action {
      // 播放与队列控制
      UiAction::TogglePlayPause
      | UiAction::PlayTrack(_)
      | UiAction::PrevTrack
      | UiAction::NextTrack
      | UiAction::PlayAllFromList(_)
      | UiAction::PlayRandomTrack
      | UiAction::ClearQueue
      | UiAction::RemoveQueueTrack(_)
      | UiAction::SetVolume(_)
      | UiAction::ToggleMute
      | UiAction::SetPlayMode(_)
      | UiAction::CycleQuality
      | UiAction::SelectQuality(_)
      | UiAction::SeekTo(_)
      | UiAction::SetEqGain(_, _)
      | UiAction::SetEqPreset(_) => {
        PlaybackService::dispatch(&self.ctx, action).await;
      }

      // 歌词操作与偏移微调
      UiAction::CopyCurrentLyric | UiAction::AdjustLyricOffset(_) => {
        LyricService::dispatch(&self.ctx, action);
      }

      // 歌单与收藏
      UiAction::OpenPlaylistDetail(_)
      | UiAction::ToggleTrackLike(_)
      | UiAction::CollectPlaylist(_)
      | UiAction::CreateNewPlaylist => {
        PlaylistService::dispatch(
          &self.ctx.client,
          &self.ctx.db,
          self.ctx.state.clone(),
          self.ctx.event_tx.clone(),
          action,
        );
      }

      // 本地音乐与下载
      UiAction::ScanLocalFolder | UiAction::SwitchLibraryTab(_) => {
        LibraryService::dispatch(
          self.ctx.db.clone(),
          self.ctx.config.clone(),
          self.ctx.event_tx.clone(),
          action,
        );
      }
      UiAction::DownloadTrack(_) | UiAction::OpenDownloadFolder => {
        DownloadService::dispatch(
          &self.ctx.db,
          &self.ctx.client,
          &self.ctx.quality_level.lock(),
          &self.ctx.event_tx,
          self.ctx.state.clone(),
          action,
        );
      }

      // 认证服务
      UiAction::OpenLoginModal
      | UiAction::RefreshQr
      | UiAction::CloseLoginModal
      | UiAction::Logout => {
        AuthService::dispatch(
          &self.ctx.client,
          self.qr_polling_cancel.clone(),
          self.ctx.event_tx.clone(),
          self.ctx.scrobble_svc.clone(),
          action,
        );
      }

      // 界面外观与主题
      UiAction::ToggleDarkMode | UiAction::SetCustomColor(_) | UiAction::ToggleDynamicColor => {
        ThemeService::dispatch(
          &self.ctx.db,
          self.ctx.state.clone(),
          *self.ctx.current_extracted_color.lock(),
          &self.ctx.event_tx,
          action,
        );
      }

      // 导航路由、弹窗与界面状态
      UiAction::NavigateTo(_)
      | UiAction::GoBack
      | UiAction::GoForward
      | UiAction::NavigateToArtist { .. }
      | UiAction::Search(_)
      | UiAction::ToggleLyricView
      | UiAction::ToggleQueueDrawer
      | UiAction::ToggleSidebar
      | UiAction::OpenEqualizer
      | UiAction::CloseEqualizer
      | UiAction::OpenSettings
      | UiAction::CloseSettings
      | UiAction::OpenComments
      | UiAction::CloseComments => {
        NavService::dispatch(&self.ctx, action);
      }
    }
  }
}
