use material_colors::color::Rgb;
use material_colors::hct::{Cam16, Hct};
use material_colors::palette::TonalPalette;
use material_colors::quantize::{Quantizer, QuantizerCelebi};
use slint::Color;

fn sample_weight(x: u32, y: u32) -> usize {
  let center = 63.0 / 2.0;
  let dx = (x as f64 - center) / center;
  let dy = (y as f64 - center) / center;
  let distance = (dx * dx + dy * dy).sqrt();
  if distance < 0.34 {
    3
  } else if distance < 0.58 {
    2
  } else {
    1
  }
}

/// 封面代表色
fn pick_representative_cover_color<'a, I>(colors: I) -> Option<Rgb>
where
  I: IntoIterator<Item = (&'a Rgb, &'a u32)>,
{
  let entries: Vec<(Rgb, u32)> = colors.into_iter().map(|(&r, &c)| (r, c)).collect();
  if entries.is_empty() {
    return None;
  }
  let total_count: u32 = entries.iter().map(|(_, cnt)| *cnt).sum();
  let mut colorful_entries: Vec<(Rgb, u32, Hct)> = Vec::new();
  let mut colorful_count = 0u32;
  for &(rgb, count) in &entries {
    let hct = Hct::new(rgb);
    if hct.get_chroma() >= 8.0 && hct.get_tone() >= 10.0 && hct.get_tone() <= 94.0 {
      colorful_count += count;
      colorful_entries.push((rgb, count, hct));
    }
  }
  if total_count == 0 || (colorful_count as f64 / total_count as f64) < 0.12 {
    return None;
  }
  let max_count = colorful_entries.iter().map(|e| e.1).max().unwrap_or(1) as f64;
  let mut best: Option<Rgb> = None;
  let mut best_score = -1.0f64;

  for (rgb, count, hct) in colorful_entries {
    let population_score = (count as f64 / max_count).powf(0.72);
    let chroma_score = (hct.get_chroma() / 52.0).min(1.0);
    let tone_score = (1.0 - (hct.get_tone() - 58.0).abs() / 58.0).max(0.0);
    let score = population_score * 0.58 + chroma_score * 0.28 + tone_score * 0.14;
    if score > best_score {
      best_score = score;
      best = Some(rgb);
    }
  }
  best
}

fn to_cover_base_color(rgb: Rgb) -> Rgb {
  let hct = Hct::new(rgb);
  let tone = hct.get_tone().clamp(28.0, 72.0);
  let chroma = hct.get_chroma().clamp(12.0, 64.0);
  let base_hct = Hct::from(hct.get_hue(), chroma, tone);
  base_hct.into()
}

/// 默认主题色
pub const DEFAULT_THEME_HEX: &str = "#fe7971";

/// 动态主题色彩服务
pub struct ThemeService;

impl ThemeService {
  pub fn soften_color(raw: Color, is_dark: bool) -> Color {
    let (r, g, b) = (raw.red(), raw.green(), raw.blue());
    let rgb = Rgb::new(r, g, b);
    let cam = Cam16::from(rgb);
    let secondary = TonalPalette::of(cam.hue, 16.0);
    let target_rgb = if is_dark {
      secondary.tone(90)
    } else {
      secondary.tone(10)
    };
    Color::from_argb_u8(255, target_rgb.red, target_rgb.green, target_rgb.blue)
  }

  /// 容器色生成 (Dark: Tone 30, Light: Tone 90)
  pub fn soften_container(raw: Color, is_dark: bool) -> Color {
    let (r, g, b) = (raw.red(), raw.green(), raw.blue());
    let rgb = Rgb::new(r, g, b);
    let cam = Cam16::from(rgb);
    let secondary = TonalPalette::of(cam.hue, 16.0);
    let target_rgb = if is_dark {
      secondary.tone(30)
    } else {
      secondary.tone(90)
    };
    Color::from_argb_u8(255, target_rgb.red, target_rgb.green, target_rgb.blue)
  }

  /// 将 HEX 颜色字符串转换为 Slint Color
  pub fn hex_to_color(hex: &str) -> Option<Color> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() == 6 {
      let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
      let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
      let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
      Some(Color::from_argb_u8(255, r, g, b))
    } else if hex.len() == 8 {
      let a = u8::from_str_radix(&hex[0..2], 16).ok()?;
      let r = u8::from_str_radix(&hex[2..4], 16).ok()?;
      let g = u8::from_str_radix(&hex[4..6], 16).ok()?;
      let b = u8::from_str_radix(&hex[6..8], 16).ok()?;
      Some(Color::from_argb_u8(a, r, g, b))
    } else {
      None
    }
  }

  /// 主题与外观领域动作统一调度入口
  pub fn dispatch(
    db: &crate::data::Database,
    state: std::sync::Arc<parking_lot::RwLock<crate::common::state::AppState>>,
    current_extracted_color: Option<[u8; 4]>,
    event_tx: &tokio::sync::mpsc::UnboundedSender<crate::common::channel::AppEvent>,
    action: crate::common::channel::UiAction,
  ) {
    match action {
      crate::common::channel::UiAction::ToggleDarkMode => {
        Self::toggle_dark_mode(db, state, event_tx);
      }
      crate::common::channel::UiAction::SetCustomColor(hex) => {
        Self::set_custom_color(db, state, event_tx, hex);
      }
      crate::common::channel::UiAction::ToggleDynamicColor => {
        Self::toggle_dynamic_color(db, state, current_extracted_color, event_tx);
      }
      _ => {}
    }
  }

  /// 切换暗色模式并持久化至 SQLite
  pub fn toggle_dark_mode(
    db: &crate::data::Database,
    state: std::sync::Arc<parking_lot::RwLock<crate::common::state::AppState>>,
    event_tx: &tokio::sync::mpsc::UnboundedSender<crate::common::channel::AppEvent>,
  ) {
    let is_dark = {
      let mut st = state.write();
      st.is_dark_mode = !st.is_dark_mode;
      st.is_dark_mode
    };
    if let Err(e) = crate::data::SettingsRepo::set(db, "dark_mode", &is_dark.to_string()) {
      tracing::warn!("持久化暗色模式设置失败: {e}");
    }
    tracing::info!("已持久化保存暗色模式到数据库: {is_dark}");
    let _ = event_tx.send(crate::common::channel::AppEvent::DarkModeChanged(is_dark));
  }

  /// 设置自定义 HEX 主题颜色并持久化至 SQLite
  pub fn set_custom_color(
    db: &crate::data::Database,
    state: std::sync::Arc<parking_lot::RwLock<crate::common::state::AppState>>,
    event_tx: &tokio::sync::mpsc::UnboundedSender<crate::common::channel::AppEvent>,
    hex: String,
  ) {
    if let Some(color) = Self::hex_to_color(&hex) {
      let formatted = if hex.starts_with('#') {
        hex
      } else {
        format!("#{hex}")
      };
      state.write().custom_color_hex = formatted.clone();
      state.write().dynamic_color_enabled = false;
      if let Err(e) = crate::data::SettingsRepo::set(db, "theme_color", &formatted) {
        tracing::warn!("持久化自定义主题色失败: {e}");
      }
      let _ = crate::data::SettingsRepo::set(db, "custom_color_hex", &formatted);
      let _ = crate::data::SettingsRepo::set(db, "dynamic_color_enabled", "false");
      tracing::info!("已持久化保存自定义主题色到数据库: {formatted}");
      let _ = event_tx.send(crate::common::channel::AppEvent::ThemeColorChanged(color));
      let _ = event_tx.send(crate::common::channel::AppEvent::CustomColorChanged(
        formatted,
      ));
      let _ = event_tx.send(crate::common::channel::AppEvent::DynamicColorChanged(false));
    }
  }

  /// 切换动态取色并持久化至 SQLite
  pub fn toggle_dynamic_color(
    db: &crate::data::Database,
    state: std::sync::Arc<parking_lot::RwLock<crate::common::state::AppState>>,
    current_extracted_color: Option<[u8; 4]>,
    event_tx: &tokio::sync::mpsc::UnboundedSender<crate::common::channel::AppEvent>,
  ) {
    let enabled = {
      let mut st = state.write();
      st.dynamic_color_enabled = !st.dynamic_color_enabled;
      st.dynamic_color_enabled
    };
    if let Err(e) =
      crate::data::SettingsRepo::set(db, "dynamic_color_enabled", &enabled.to_string())
    {
      tracing::warn!("持久化自适应取色开关失败: {e}");
    }
    tracing::info!("已持久化保存自适应取色开关到数据库: {enabled}");
    let _ = event_tx.send(crate::common::channel::AppEvent::DynamicColorChanged(
      enabled,
    ));
    if enabled && let Some([a, r, g, b]) = current_extracted_color {
      let accent = Color::from_argb_u8(a, r, g, b);
      let _ = event_tx.send(crate::common::channel::AppEvent::ThemeColorChanged(accent));
    }
  }

  /// 根据专辑封面图像通过 SPlayer Celebi 量化与评分算法提取基色 (Material 3 Dynamic Color)
  pub fn extract_cover_accent(_track_id: &str, cover_hint: Option<&[u8]>) -> Color {
    if let Some(bytes) = cover_hint
      && let Ok(img) = image::load_from_memory(bytes)
    {
      let resized = img
        .resize_exact(64, 64, image::imageops::FilterType::Triangle)
        .to_rgba8();
      let mut pixels = Vec::with_capacity(64 * 64 * 2);
      for y in 3..61 {
        for x in 3..61 {
          let p = resized.get_pixel(x, y);
          if p[3] < 16 {
            continue;
          }
          let rgb = Rgb::new(p[0], p[1], p[2]);
          let w = sample_weight(x, y);
          for _ in 0..w {
            pixels.push(rgb);
          }
        }
      }

      if !pixels.is_empty() {
        let quantized = QuantizerCelebi::quantize(&pixels, 128);
        if let Some(picked) = pick_representative_cover_color(&quantized.color_to_count) {
          let base_rgb = to_cover_base_color(picked);
          return Color::from_argb_u8(255, base_rgb.red, base_rgb.green, base_rgb.blue);
        }
      }
    }

    // 回退机制：默认唯一预设主题色 #fe7971
    Self::hex_to_color(DEFAULT_THEME_HEX).unwrap_or(Color::from_argb_u8(255, 254, 121, 113))
  }
}
