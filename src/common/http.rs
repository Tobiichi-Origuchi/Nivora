use std::sync::OnceLock;
use std::time::Duration;

#[cfg(target_os = "linux")]
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
#[cfg(target_os = "windows")]
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Safari/537.36 Chrome/91.0.4472.164 NeteaseMusicDesktop/3.0.18.203152";
#[cfg(target_os = "macos")]
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36 Edg/124.0.0.0";
#[cfg(target_os = "android")]
pub const DEFAULT_USER_AGENT: &str = "NeteaseMusic/9.1.65.240927161425(9001065);Dalvik/2.1.0 (Linux; U; Android 14; 23013RK75C Build/UKQ1.230804.001)";
#[cfg(target_os = "ios")]
pub const DEFAULT_USER_AGENT: &str = "NeteaseMusic 9.0.90/5038 (iPhone; iOS 16.2; zh_CN)";

pub struct HttpClientPool;

static GLOBAL_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

impl HttpClientPool {
  pub fn get_client() -> &'static reqwest::Client {
    GLOBAL_HTTP_CLIENT.get_or_init(|| {
      reqwest::Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .tcp_keepalive(Some(Duration::from_secs(60)))
        .pool_idle_timeout(Some(Duration::from_secs(90)))
        .pool_max_idle_per_host(10)
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
    })
  }
}

/// 国内随机公共 IP 地址池
pub const CHINESE_IP_POOL: &[&str] = &[
  "114.114.114.114",
  "116.25.146.177",
  "183.14.135.122",
  "220.181.38.148",
  "123.125.114.144",
  "180.149.132.47",
  "202.108.22.5",
  "61.135.169.121",
  "218.30.118.6",
  "121.14.77.221",
];

/// 随机获取伪装国内 IP
pub fn get_random_disguise_ip() -> &'static str {
  use rand::seq::SliceRandom;
  let mut rng = rand::thread_rng();
  CHINESE_IP_POOL
    .choose(&mut rng)
    .unwrap_or(&"116.25.146.177")
}

/// 为网易云等图片 URL 附加上尺寸参数 (param={size}y{size})
pub fn with_pic_size(url: Option<&str>, size: u32) -> Option<String> {
  let url = url?.trim();
  if url.is_empty() {
    return None;
  }
  // 非网络链接或已带尺寸参数则不处理
  if !url.starts_with("http://") && !url.starts_with("https://") {
    return Some(url.to_string());
  }
  if url.contains("param=") {
    return Some(url.to_string());
  }
  let sep = if url.contains('?') { '&' } else { '?' };
  Some(format!("{}{sep}param={}y{}", url, size, size))
}
