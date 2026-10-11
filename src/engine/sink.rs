use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use std::sync::Arc;

use crate::common::error::{AppError, AppResult};

/// 音频硬件输出端包装
pub struct AudioSink {
  device_sink: MixerDeviceSink,
  player: Arc<Player>,
}

impl AudioSink {
  /// 追加流式音频源到输出硬件缓冲
  pub fn append_source<S>(&self, source: S)
  where
    S: Source<Item = f32> + Send + 'static,
  {
    self.player.append(source);
  }

  /// 淡入淡出
  pub fn play_crossfade<S>(&mut self, source: S, target_volume: f32, crossfade_ms: u64)
  where
    S: Source<Item = f32> + Send + 'static,
  {
    let new_player = Arc::new(Player::connect_new(self.device_sink.mixer()));
    let old_player = self.player.clone();
    let was_playing = !old_player.is_paused() && !old_player.empty();
    let clamped_target = target_volume.clamp(0.0, 1.5);

    if was_playing && crossfade_ms > 0 {
      new_player.set_volume(clamped_target);
      new_player.append(source);
      new_player.play();
      self.player = new_player;
    } else {
      old_player.stop();
      new_player.set_volume(clamped_target);
      new_player.append(source);
      new_player.play();
      self.player = new_player;
    }
  }

  /// 初始化默认硬件音频输出设备
  pub fn try_new() -> AppResult<Self> {
    let device_sink = DeviceSinkBuilder::open_default_sink()
      .map_err(|e| AppError::AudioEngine(format!("无法打开系统默认音频输出设备: {e}")))?;

    let player = Player::connect_new(device_sink.mixer());

    Ok(Self {
      device_sink,
      player: Arc::new(player),
    })
  }

  /// 播放已加入缓冲区的音频
  pub fn play(&self) {
    self.player.play();
  }

  /// 暂停播放
  pub fn pause(&self) {
    self.player.pause();
  }

  /// 停止播放并清空当前缓冲区
  pub fn stop(&self) {
    self.player.stop();
  }

  /// 当前是否处于暂停状态
  pub fn is_paused(&self) -> bool {
    self.player.is_paused()
  }

  /// 设置硬件音量 (0.0 ~ 1.5)
  pub fn set_volume(&self, volume: f32) {
    self.player.set_volume(volume.clamp(0.0, 1.5));
  }

  /// 获取当前音量
  pub fn volume(&self) -> f32 {
    self.player.volume()
  }

  /// 当前播放队列是否已全部放完
  pub fn empty(&self) -> bool {
    self.player.empty()
  }
}
