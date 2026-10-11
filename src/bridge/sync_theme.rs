use slint::{Color, ComponentHandle};

use crate::bridge::{MainWindow, Theme};
use crate::service::ThemeService;

/// 主题与调色板 UI 桥接同步器
pub struct SyncTheme;

impl SyncTheme {
  /// 将由 Google Material Design 3 CAM16 计算的柔化调色板单向推送至 Slint UI
  pub fn sync_theme(ui: &MainWindow, raw: Color, is_dark: bool) {
    let theme = ui.global::<Theme>();
    let softened = ThemeService::soften_color(raw, is_dark);
    let container = ThemeService::soften_container(raw, is_dark);
    theme.set_raw_primary(raw);
    theme.set_primary(softened);
    theme.set_primary_container(container);
  }
}
