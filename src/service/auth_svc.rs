use qrcode::QrCode;
use qrcode::render::svg;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;

use crate::api::NcmClient;
use crate::api::auth::QrCodeStatus;
use crate::common::channel::{AppEvent, UiAction};
use crate::common::error::{AppError, AppResult};

/// 认证与账号服务
pub struct AuthService;

impl AuthService {
  /// 生成二维码图像 SVG 字符串
  pub fn generate_qr_svg(content: &str) -> AppResult<String> {
    let code = QrCode::new(content.as_bytes())
      .map_err(|e| AppError::Network(format!("生成二维码失败: {}", e)))?;

    let svg_str = code.render::<svg::Color>().min_dimensions(200, 200).build();

    Ok(svg_str)
  }

  /// 认证领域动作统一调度入口
  pub fn dispatch(
    client: &NcmClient,
    cancel_token: Arc<AtomicBool>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    action: UiAction,
  ) {
    match action {
      UiAction::OpenLoginModal | UiAction::RefreshQr => {
        Self::open_or_refresh_qr(client, cancel_token, event_tx);
      }
      UiAction::CloseLoginModal => {
        Self::close_login_modal(&cancel_token, &event_tx);
      }
      UiAction::Logout => {
        Self::logout(client, event_tx);
      }
      _ => {}
    }
  }

  /// 打开或刷新扫码登录二维码
  pub fn open_or_refresh_qr(
    client: &NcmClient,
    cancel_token: Arc<AtomicBool>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
  ) {
    info!("生成扫码登录二维码");
    let _ = event_tx.send(AppEvent::LoginModalToggled(true));
    cancel_token.store(true, Ordering::SeqCst);

    let client = client.clone();
    let ev_tx = event_tx.clone();
    let cancel = cancel_token.clone();

    tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(50)).await;
      cancel.store(false, Ordering::SeqCst);

      let _ = ev_tx.send(AppEvent::QrStatusChanged(
        "正在生成登录二维码...".to_string(),
      ));
      match client.auth().create_qr_key().await {
        Ok(key) => {
          let qr_url = client.auth().get_qr_url(&key);
          match Self::generate_qr_svg(&qr_url) {
            Ok(svg) => {
              let _ = ev_tx.send(AppEvent::QrCodeGenerated {
                svg_data: svg,
                key: key.clone(),
              });
              let _ = ev_tx.send(AppEvent::QrStatusChanged(
                "请使用网易云音乐 App 扫码登录".to_string(),
              ));

              // 启动 2s 轮询任务 (最多 150 次 = 300 秒超时保护)
              let mut poll_count = 0;
              loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if cancel.load(Ordering::SeqCst) {
                  break;
                }
                poll_count += 1;
                if poll_count >= 150 {
                  let _ = ev_tx.send(AppEvent::QrStatusChanged(
                    "二维码已过期，请点击刷新".to_string(),
                  ));
                  break;
                }

                match client.auth().check_qr_status(&key).await {
                  Ok(QrCodeStatus::WaitingForScan) => {
                    let _ = ev_tx.send(AppEvent::QrStatusChanged(
                      "请使用网易云音乐 App 扫码登录".to_string(),
                    ));
                  }
                  Ok(QrCodeStatus::WaitingForConfirm) => {
                    let _ = ev_tx.send(AppEvent::QrStatusChanged(
                      "已扫码，请在手机端确认授权".to_string(),
                    ));
                  }
                  Ok(QrCodeStatus::Success) => {
                    let _ = ev_tx.send(AppEvent::QrStatusChanged(
                      "授权成功，正在同步账号资料...".to_string(),
                    ));
                    if let Ok(Some(profile)) = client.auth().get_login_status().await {
                      let _ = ev_tx.send(AppEvent::UserChanged(Some(profile)));
                    }
                    tokio::time::sleep(Duration::from_millis(600)).await;
                    let _ = ev_tx.send(AppEvent::LoginModalToggled(false));
                    break;
                  }
                  Ok(QrCodeStatus::Expired) => {
                    let _ = ev_tx.send(AppEvent::QrStatusChanged(
                      "二维码已过期，请点击刷新".to_string(),
                    ));
                    break;
                  }
                  _ => {}
                }
              }
            }
            Err(e) => {
              tracing::warn!("渲染二维码失败: {e}");
              let _ = ev_tx.send(AppEvent::QrStatusChanged(
                "生成二维码图像失败，请点击刷新".to_string(),
              ));
            }
          }
        }
        Err(e) => {
          tracing::warn!("创建登录二维码 Key 失败: {e}");
          let _ = ev_tx.send(AppEvent::QrStatusChanged(
            "获取二维码失败，请点击刷新".to_string(),
          ));
        }
      }
    });
  }

  /// 关闭扫码登录弹窗
  pub fn close_login_modal(cancel_token: &AtomicBool, event_tx: &mpsc::UnboundedSender<AppEvent>) {
    cancel_token.store(true, Ordering::SeqCst);
    let _ = event_tx.send(AppEvent::LoginModalToggled(false));
  }

  /// 注销登录
  pub fn logout(client: &NcmClient, event_tx: mpsc::UnboundedSender<AppEvent>) {
    info!("用户注销登录");
    let client = client.clone();
    tokio::spawn(async move {
      let _ = client.auth().logout().await;
      let _ = event_tx.send(AppEvent::UserChanged(None));
    });
  }
}
