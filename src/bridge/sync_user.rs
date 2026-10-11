use slint::ComponentHandle;

use crate::bridge::AuthState;
use crate::bridge::MainWindow;
use crate::common::model::UserProfile;

/// 用户资料与账号登录认证状态同步器
pub struct SyncUser;

impl SyncUser {
  /// 同步用户资料与登录标识
  pub fn sync_user(ui: &MainWindow, user_opt: Option<&UserProfile>) {
    let global = ui.global::<AuthState>();
    if let Some(user) = user_opt {
      global.set_is_logged_in(true);
      global.set_nickname(user.nickname.clone().into());
      global.set_avatar_url(user.avatar_url.clone().into());
      global.set_vip_level(user.vip_type);
      global.set_is_login_dialog_open(false);
    } else {
      global.set_is_logged_in(false);
      global.set_nickname("立即登录".into());
      global.set_avatar_url("".into());
      global.set_vip_level(0);
    }
  }

  /// 同步扫码二维码矢量图像
  pub fn sync_qr_image(ui: &MainWindow, svg_str: &str) {
    if let Ok(img) = slint::Image::load_from_svg_data(svg_str.as_bytes()) {
      ui.global::<AuthState>().set_qr_code_image(img);
    }
  }

  /// 同步二维码扫码状态提示文字
  pub fn sync_qr_status(ui: &MainWindow, text: &str) {
    ui.global::<AuthState>().set_qr_status_text(text.into());
  }

  /// 同步登录弹窗显隐状态
  pub fn sync_login_modal(ui: &MainWindow, is_open: bool) {
    ui.global::<AuthState>().set_is_login_dialog_open(is_open);
  }
}
