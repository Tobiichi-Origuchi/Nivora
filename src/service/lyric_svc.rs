use serde::{Deserialize, Serialize};

use crate::api::NcmClient;
use crate::common::channel::{AppEvent, UiAction};
use crate::data::{Database, LyricRepo};
use crate::service::context::AppContext;

const PLATFORM_NETEASE: &str = "netease";

#[derive(Debug, Serialize, Deserialize)]
struct CachedLyrics {
  lrc: Option<String>,
  yrc: Option<String>,
  tlyric: Option<String>,
}

/// 歌词展示、剪贴板与时间轴微调服务
pub struct LyricService;

impl LyricService {
  /// 加载歌词：优先从本地数据库缓存读取，未命中则请求网络并写入本地持久化缓存
  pub async fn load_lyrics(
    client: &NcmClient,
    db: &Database,
    song_id: &str,
  ) -> (Option<String>, Option<String>, Option<String>) {
    // 1. 尝试从本地缓存中提取
    let db_c = db.clone();
    let sid = song_id.to_string();
    if let Ok(Ok(Some(cached_json))) =
      tokio::task::spawn_blocking(move || LyricRepo::get_lyric(&db_c, PLATFORM_NETEASE, &sid)).await
      && let Ok(cached) = serde_json::from_str::<CachedLyrics>(&cached_json)
    {
      return (cached.lrc, cached.yrc, cached.tlyric);
    }

    // 2. 本地未命中，发起远程网络拉取
    if let Ok((lrc, yrc, tlyric)) = client.lyric().get_lyrics(song_id).await {
      // 3. 将成功拉取的歌词写入本地持久化缓存
      let cache_payload = CachedLyrics {
        lrc: lrc.clone(),
        yrc: yrc.clone(),
        tlyric: tlyric.clone(),
      };
      if let Ok(json_str) = serde_json::to_string(&cache_payload) {
        let db_c = db.clone();
        let sid = song_id.to_string();
        tokio::task::spawn_blocking(move || {
          let _ = LyricRepo::save_lyric(&db_c, PLATFORM_NETEASE, &sid, &json_str);
        });
      }
      return (lrc, yrc, tlyric);
    }

    (None, None, None)
  }

  /// 歌词动作统一调度入口
  pub fn dispatch(ctx: &AppContext, action: UiAction) {
    match action {
      UiAction::CopyCurrentLyric => Self::copy_current_lyric(ctx),
      UiAction::AdjustLyricOffset(sec) => Self::adjust_lyric_offset(ctx, sec),
      _ => {}
    }
  }

  /// 复制当前正在播放的歌词文本到系统剪贴板
  pub fn copy_current_lyric(ctx: &AppContext) {
    let cur_line = {
      let tl = ctx.lyric_timeline.read();
      let pos = ctx.player.lock().position_ms();
      tl.find_active_index(pos)
        .and_then(|idx| tl.lines().get(idx))
        .cloned()
    };
    if let Some(line) = cur_line {
      let text = line.text().to_string();
      let msg = if crate::os::set_clipboard_text(&text) {
        "已复制当前歌词到剪贴板"
      } else {
        "复制歌词失败，无法访问系统剪贴板"
      };
      let _ = ctx.event_tx.send(AppEvent::ShowToast(msg.to_string()));
    } else {
      let _ = ctx
        .event_tx
        .send(AppEvent::ShowToast("暂无正在播放的歌词".to_string()));
    }
  }

  /// 微调歌词时间轴偏移 (秒)
  pub fn adjust_lyric_offset(ctx: &AppContext, sec: f32) {
    ctx
      .lyric_timeline
      .write()
      .set_offset_ms((sec * 1000.0).round() as i64);
  }
}
