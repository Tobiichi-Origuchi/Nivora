use ncm_api_rs::ApiClient;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::Arc;

use super::client::{create_disguised_query, request_with_retry};
use super::session::SessionManager;
use crate::common::error::{AppError, AppResult};
use crate::common::model::UserProfile;

/// 扫码登录状态码
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrCodeStatus {
  /// 800: 二维码不存在或已过期
  Expired,
  /// 801: 等待用户扫码
  WaitingForScan,
  /// 802: 待用户在手机端确认授权
  WaitingForConfirm,
  /// 803: 授权登录成功并已下发凭据
  Success,
  /// 未知或未处理的状态码
  Unknown(i64),
}

/// 认证与登录网络 API
#[derive(Clone)]
pub struct AuthApi {
  session: SessionManager,
  client: Arc<RwLock<ApiClient>>,
}

impl AuthApi {
  pub fn new(session: SessionManager, client: Arc<RwLock<ApiClient>>) -> Self {
    Self { session, client }
  }

  /// 获取底层会话管理器引用
  pub fn session(&self) -> &SessionManager {
    &self.session
  }

  /// 创建登录二维码 Key
  pub async fn create_qr_key(&self) -> AppResult<String> {
    let client = self.client.clone();
    let query = create_disguised_query();
    let resp = request_with_retry(|| {
      let client = client.read().clone();
      let query = query.clone();
      async move { client.login_qr_key(&query).await }
    })
    .await?;

    let unikey = resp
      .body
      .get("unikey")
      .or_else(|| resp.body.pointer("/data/unikey"))
      .and_then(Value::as_str)
      .ok_or_else(|| {
        AppError::Network(format!(
          "未能从网易云返回中获取二维码 unikey, 响应体: {}",
          resp.body
        ))
      })?;

    Ok(unikey.to_string())
  }

  /// 获取二维码对应的扫码授权 URL
  pub fn get_qr_url(&self, key: &str) -> String {
    format!("https://music.163.com/login?codekey={}", key)
  }

  /// 轮询检测二维码状态 (800 / 801 / 802 / 803)
  pub async fn check_qr_status(&self, key: &str) -> AppResult<QrCodeStatus> {
    let client = self.client.read().clone();
    let query = create_disguised_query().param("key", key);
    let resp = client.login_qr_check(&query).await?;

    // 同步可能的响应头 Cookie
    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    let code = resp.body.get("code").and_then(Value::as_i64).unwrap_or(801);
    match code {
      800 => Ok(QrCodeStatus::Expired),
      801 => Ok(QrCodeStatus::WaitingForScan),
      802 => Ok(QrCodeStatus::WaitingForConfirm),
      803 => {
        // 授权成功，提取下发的 Cookie 字符串
        if let Some(cookie_str) = resp.body.get("cookie").and_then(Value::as_str)
          && !cookie_str.is_empty()
        {
          let _ = self.session.set_cookie(cookie_str);
        }
        Ok(QrCodeStatus::Success)
      }
      other => Ok(QrCodeStatus::Unknown(other)),
    }
  }

  /// 获取当前登录账号的用户资料
  pub async fn get_login_status(&self) -> AppResult<Option<UserProfile>> {
    let client = self.client.read().clone();
    let query = create_disguised_query();
    let resp = match client.login_status(&query).await {
      Ok(r) => r,
      Err(_) => return Ok(None),
    };

    if !resp.cookie.is_empty() {
      let _ = self.session.merge_cookies(&resp.cookie);
    }

    // 解析响应中的 profile 与 account
    let body = &resp.body;
    let profile_obj = body
      .pointer("/data/profile")
      .or_else(|| body.get("profile"));
    let account_obj = body
      .pointer("/data/account")
      .or_else(|| body.get("account"));

    let user_id = profile_obj
      .and_then(|p| p.get("userId"))
      .and_then(Value::as_i64)
      .or_else(|| {
        account_obj
          .and_then(|a| a.get("id"))
          .and_then(Value::as_i64)
      });

    let Some(uid) = user_id else {
      return Ok(None);
    };

    let nickname = profile_obj
      .and_then(|p| p.get("nickname"))
      .and_then(Value::as_str)
      .unwrap_or("云音乐用户")
      .to_string();

    let avatar_url = profile_obj
      .and_then(|p| p.get("avatarUrl"))
      .and_then(Value::as_str)
      .unwrap_or("")
      .to_string();

    let vip_type = profile_obj
      .and_then(|p| p.get("vipType"))
      .and_then(Value::as_i64)
      .unwrap_or(0) as i32;

    Ok(Some(UserProfile {
      user_id: uid,
      nickname,
      avatar_url,
      vip_type,
      is_anonymous: false,
    }))
  }

  /// 退出登录并清除本地存储的会话凭据
  pub async fn logout(&self) -> AppResult<()> {
    let client = self.client.read().clone();
    let query = create_disguised_query();
    let _ = client.logout(&query).await;
    self.session.clear_session()?;
    Ok(())
  }
}
