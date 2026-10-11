use tracing::{error, info};

pub fn set_clipboard_text(text: &str) -> bool {
  match arboard::Clipboard::new() {
    Ok(mut clipboard) => {
      if let Err(e) = clipboard.set_text(text) {
        error!("写入剪贴板失败: {e}");
        false
      } else {
        info!("已复制到剪贴板 ({} 字符)", text.len());
        true
      }
    }
    Err(e) => {
      error!("无法初始化剪贴板句柄: {e}");
      false
    }
  }
}

pub fn get_clipboard_text() -> Option<String> {
  match arboard::Clipboard::new() {
    Ok(mut clipboard) => clipboard.get_text().ok(),
    Err(e) => {
      error!("无法读取剪贴板: {e}");
      None
    }
  }
}
