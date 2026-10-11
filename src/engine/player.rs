use rodio::Source;
use std::io::{Seek, SeekFrom};
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use super::decoder::AudioDecoder;
use super::equalizer::{EqPreset, Equalizer, EqualizerParams};
use super::loudness::OutputLimiter;
use super::sink::AudioSink;
use super::source::AudioSourceStream;
use super::tempo::{TempoParams, TempoProcessor};
use super::visualizer::{FftVisualizer, SPECTRUM_BANDS};
use crate::common::error::{AppError, AppResult};
use crate::common::types::PlayerState;

const DEFAULT_CHANNELS: NonZeroU16 = match NonZeroU16::new(2) {
  Some(v) => v,
  None => unreachable!(),
};
const DEFAULT_SAMPLE_RATE: NonZeroU32 = match NonZeroU32::new(44100) {
  Some(v) => v,
  None => unreachable!(),
};

/// 流式音频源
pub struct StreamingAudioSource {
  decoder: AudioDecoder,
  buffer: Vec<f32>,
  buffer_pos: usize,

  equalizer_params: Arc<EqualizerParams>,
  equalizer: Equalizer,
  limiter: OutputLimiter,
  tempo_params: Arc<TempoParams>,
  tempo: TempoProcessor,
  visualizer: Arc<FftVisualizer>,

  samples_consumed: Arc<AtomicU64>,
  sample_rate: u32,
  channels: u16,
  duration: Option<Duration>,
  is_eof: bool,

  // 样点级数字淡入渐变包络
  ramp_frames_remaining: usize,
  ramp_total_frames: usize,

  // 样点级数字淡出信号与包络
  fade_out_signal: Arc<AtomicU64>,
  fade_out_frames_remaining: Option<usize>,
  fade_out_total_frames: usize,

  current_chunk_input_len: usize,
  current_chunk_output_len: usize,
  chunk_base_input_samples: u64,
}

impl StreamingAudioSource {
  pub fn new(
    decoder: AudioDecoder,
    equalizer_params: Arc<EqualizerParams>,
    tempo_params: Arc<TempoParams>,
    visualizer: Arc<FftVisualizer>,
    samples_consumed: Arc<AtomicU64>,
    fade_in_ms: u64,
    fade_out_signal: Arc<AtomicU64>,
  ) -> Self {
    let sample_rate = decoder.sample_rate();
    let channels = decoder.channels();
    let duration = if decoder.duration_ms() > 0 {
      Some(Duration::from_millis(decoder.duration_ms()))
    } else {
      None
    };

    let ramp_total_frames = if fade_in_ms > 0 {
      (sample_rate as usize * channels as usize * fade_in_ms as usize) / 1000
    } else {
      0
    };
    let chunk_base_input_samples = samples_consumed.load(Ordering::Relaxed);

    let mut equalizer = Equalizer::new(sample_rate as f32);
    equalizer.sync_with_params(&equalizer_params);

    let mut tempo = TempoProcessor::new();
    tempo.sync_with_params(&tempo_params);

    Self {
      decoder,
      buffer: Vec::new(),
      buffer_pos: 0,
      equalizer_params,
      equalizer,
      limiter: OutputLimiter::new(),
      tempo_params,
      tempo,
      visualizer,
      samples_consumed,
      sample_rate,
      channels,
      duration,
      is_eof: false,
      ramp_frames_remaining: ramp_total_frames,
      ramp_total_frames,
      fade_out_signal,
      fade_out_frames_remaining: None,
      fade_out_total_frames: 0,
      current_chunk_input_len: 0,
      current_chunk_output_len: 0,
      chunk_base_input_samples,
    }
  }
}

impl Iterator for StreamingAudioSource {
  type Item = f32;

  fn next(&mut self) -> Option<Self::Item> {
    while self.buffer_pos >= self.buffer.len() {
      if self.is_eof {
        return None;
      }

      // 检查外部淡出请求
      if self.fade_out_frames_remaining.is_none() {
        let sig = self.fade_out_signal.load(Ordering::Relaxed);
        if sig > 0 {
          self.fade_out_total_frames = sig as usize;
          self.fade_out_frames_remaining = Some(sig as usize);
        }
      }

      match self.decoder.next_chunk() {
        Ok(Some(mut chunk)) => {
          if chunk.is_empty() {
            continue;
          }

          let raw_input_len = chunk.len();

          // 1. 变速变调处理
          self.tempo.sync_with_params(&self.tempo_params);
          if (self.tempo.speed() - 1.0).abs() > 0.01 || self.tempo.pitch_semitones().abs() > 0.01 {
            let processed =
              self
                .tempo
                .process_interleaved(&chunk, self.channels as usize, self.sample_rate);
            chunk.clear();
            chunk.extend_from_slice(processed);
          }
          let output_len = chunk.len();

          self.chunk_base_input_samples += self.current_chunk_input_len as u64;
          self.current_chunk_input_len = raw_input_len;
          self.current_chunk_output_len = output_len;

          // 2. 均衡器处理
          self.equalizer.sync_with_params(&self.equalizer_params);
          self.equalizer.set_sample_rate(self.sample_rate as f32);
          self.equalizer.process(&mut chunk);

          // 3. 峰值限幅保护
          self.limiter.process(&mut chunk);

          // 4. 起始音量平滑淡入渐变包络
          if self.ramp_frames_remaining > 0 && self.ramp_total_frames > 0 {
            let ch = self.channels as usize;
            let frames = chunk.len() / ch;
            for f in 0..frames {
              if self.ramp_frames_remaining > 0 {
                let factor =
                  1.0 - (self.ramp_frames_remaining as f32 / self.ramp_total_frames as f32);
                for c in 0..ch {
                  chunk[f * ch + c] *= factor;
                }
                self.ramp_frames_remaining -= 1;
              }
            }
          }

          // 5. 跨曲目平滑淡出渐变包络
          if let Some(ref mut remaining) = self.fade_out_frames_remaining {
            let ch = self.channels as usize;
            let frames = chunk.len() / ch;
            let mut reached_eof = false;
            for f in 0..frames {
              if *remaining == 0 {
                chunk.truncate(f * ch);
                reached_eof = true;
                break;
              }
              let factor = *remaining as f32 / self.fade_out_total_frames as f32;
              for c in 0..ch {
                chunk[f * ch + c] *= factor;
              }
              *remaining -= 1;
            }
            if reached_eof {
              self.is_eof = true;
              if chunk.is_empty() {
                return None;
              }
            }
          }

          // 6. 注入实时音频采样至 FFT 频谱分析器
          self.visualizer.push_samples(&chunk, self.channels as usize);

          self.buffer = chunk;
          self.buffer_pos = 0;
        }
        Ok(None) => {
          self.is_eof = true;
          return None;
        }
        Err(e) => {
          tracing::warn!("解码流中断: {e}");
          self.is_eof = true;
          return None;
        }
      }
    }

    let s = self.buffer[self.buffer_pos];
    self.buffer_pos += 1;
    let consumed_in_chunk = if self.current_chunk_output_len > 0 {
      (self.buffer_pos as u64 * self.current_chunk_input_len as u64)
        / self.current_chunk_output_len as u64
    } else {
      0
    };
    self.samples_consumed.store(
      self.chunk_base_input_samples + consumed_in_chunk,
      Ordering::Relaxed,
    );
    Some(s)
  }
}

impl Source for StreamingAudioSource {
  fn current_span_len(&self) -> Option<usize> {
    Some(self.buffer.len().saturating_sub(self.buffer_pos))
  }

  fn channels(&self) -> NonZeroU16 {
    NonZeroU16::new(self.channels).unwrap_or(DEFAULT_CHANNELS)
  }

  fn sample_rate(&self) -> NonZeroU32 {
    NonZeroU32::new(self.sample_rate).unwrap_or(DEFAULT_SAMPLE_RATE)
  }

  fn total_duration(&self) -> Option<Duration> {
    self.duration
  }
}

/// 音频播放器状态机
pub struct InnerPlayer {
  sink: Option<AudioSink>,
  current_source: Option<AudioSourceStream>,
  current_decoder: Option<AudioDecoder>,
  current_fade_out_signal: Option<Arc<AtomicU64>>,

  equalizer_params: Arc<EqualizerParams>,
  tempo_params: Arc<TempoParams>,
  visualizer: Arc<FftVisualizer>,

  state: PlayerState,
  volume: f32,
  fade_duration_ms: u64,

  samples_consumed: Arc<AtomicU64>,
  sample_rate: Arc<AtomicU32>,
  duration_ms: Arc<AtomicU64>,
  paused_position_ms: u64,

  load_token: Arc<AtomicU64>,
}

impl Default for InnerPlayer {
  fn default() -> Self {
    Self::new()
  }
}

impl InnerPlayer {
  pub fn new() -> Self {
    let sink = AudioSink::try_new().ok();
    Self {
      sink,
      current_source: None,
      current_decoder: None,
      current_fade_out_signal: None,
      equalizer_params: Arc::new(EqualizerParams::new()),
      tempo_params: Arc::new(TempoParams::new()),
      visualizer: Arc::new(FftVisualizer::new()),
      state: PlayerState::Idle,
      volume: 0.8,
      fade_duration_ms: 300,
      samples_consumed: Arc::new(AtomicU64::new(0)),
      sample_rate: Arc::new(AtomicU32::new(44100)),
      duration_ms: Arc::new(AtomicU64::new(0)),
      paused_position_ms: 0,
      load_token: Arc::new(AtomicU64::new(0)),
    }
  }

  /// 获取底层音频硬件输出
  fn ensure_sink(&mut self) -> AppResult<&mut AudioSink> {
    if self.sink.is_none() {
      self.sink = Some(AudioSink::try_new()?);
    }
    Ok(self.sink.as_mut().unwrap())
  }

  /// 设置跨曲目淡入淡出过渡时长 (毫秒)
  pub fn set_fade_duration(&mut self, ms: u64) {
    self.fade_duration_ms = ms;
  }

  /// 获取跨曲目淡入淡出过渡时长 (毫秒)
  pub fn fade_duration(&self) -> u64 {
    self.fade_duration_ms
  }

  /// 加载新音源并准备流式播放
  pub fn load_source(&mut self, source: AudioSourceStream, auto_play: bool) -> AppResult<u64> {
    let token = self.load_token.fetch_add(1, Ordering::SeqCst) + 1;
    self.state = PlayerState::Loading;

    // 若旧音频正在播放，向其发送淡出信号；并主动取消其后台网络缓冲任务
    if let Some(sig) = self.current_fade_out_signal.take() {
      let sr = self.sample_rate.load(Ordering::Relaxed).max(1);
      let frames = (sr as usize * 2 * self.fade_duration_ms as usize) / 1000;
      sig.store(frames.max(1) as u64, Ordering::SeqCst);
    }
    if let Some(prev) = self.current_source.take() {
      prev.cancel();
    }
    if !auto_play && let Some(sink) = self.sink.as_ref() {
      sink.stop();
    }

    // 保留克隆流用于后续 seek
    let source_for_seek = source.try_clone().ok();
    let decoder = AudioDecoder::new(source)?;
    let dur = decoder.duration_ms();
    let sample_rate = decoder.sample_rate();

    // 竞态检测
    if self.load_token.load(Ordering::SeqCst) != token {
      tracing::info!("加载请求已被新的切歌令牌取代，丢弃本次加载");
      return Ok(0);
    }

    self.sample_rate.store(sample_rate, Ordering::SeqCst);
    self.duration_ms.store(dur, Ordering::SeqCst);
    self.samples_consumed.store(0, Ordering::SeqCst);
    self.paused_position_ms = 0;
    self.current_source = source_for_seek;
    self.current_decoder = Some(decoder);

    if auto_play {
      self.play_internal(0, true)?;
    } else {
      self.state = PlayerState::Paused;
    }

    Ok(dur)
  }

  fn play_internal(&mut self, start_offset_ms: u64, crossfade: bool) -> AppResult<()> {
    let vol = self.volume;
    self.ensure_sink()?;

    let mut decoder = if let Some(d) = self.current_decoder.take() {
      d
    } else {
      let mut source_clone = self
        .current_source
        .as_ref()
        .ok_or_else(|| AppError::AudioEngine("未加载任何可播放音频".to_string()))?
        .try_clone()
        .map_err(|e| AppError::AudioEngine(format!("音源复用失败: {e}")))?;
      let _ = source_clone.seek(SeekFrom::Start(0));
      AudioDecoder::new(source_clone)?
    };

    let sample_rate = decoder.sample_rate();
    self.sample_rate.store(sample_rate, Ordering::SeqCst);

    if start_offset_ms > 0 {
      let _ = decoder.seek_ms(start_offset_ms);
    }

    let initial_samples = (start_offset_ms as f64 / 1000.0 * sample_rate as f64 * 2.0) as u64;
    let consumed = Arc::new(AtomicU64::new(initial_samples));
    self.samples_consumed = consumed.clone();
    self.paused_position_ms = start_offset_ms;

    let fade_out_signal = Arc::new(AtomicU64::new(0));
    self.current_fade_out_signal = Some(fade_out_signal.clone());

    let fade_in_ms = if crossfade { self.fade_duration_ms } else { 20 };

    let streaming_source = StreamingAudioSource::new(
      decoder,
      self.equalizer_params.clone(),
      self.tempo_params.clone(),
      self.visualizer.clone(),
      consumed,
      fade_in_ms,
      fade_out_signal,
    );

    let fade_ms = self.fade_duration_ms;
    if let Some(sink) = self.sink.as_mut() {
      if crossfade && fade_ms > 0 {
        sink.play_crossfade(streaming_source, vol, fade_ms);
      } else {
        sink.stop();
        sink.set_volume(vol);
        sink.append_source(streaming_source);
        sink.play();
      }
    }

    self.state = PlayerState::Playing;
    Ok(())
  }

  /// 开始或恢复播放
  pub fn play(&mut self) -> AppResult<()> {
    if self.state == PlayerState::Playing {
      return Ok(());
    }

    if let Some(sink) = self.sink.as_ref()
      && sink.is_paused()
    {
      sink.play();
      self.state = PlayerState::Playing;
      return Ok(());
    }

    let current_pos = self.paused_position_ms;
    self.play_internal(current_pos, false)
  }

  /// 暂停播放
  pub fn pause(&mut self) -> AppResult<()> {
    if self.state != PlayerState::Playing {
      return Ok(());
    }

    if let Some(sink) = self.sink.as_ref() {
      sink.pause();
    }

    self.paused_position_ms = self.position_ms();
    self.state = PlayerState::Paused;
    Ok(())
  }

  /// 跳转播放进度 (毫秒)
  pub fn seek(&mut self, target_ms: u64) -> AppResult<()> {
    let dur = self.duration_ms.load(Ordering::SeqCst);
    let safe_target = if dur > 0 {
      target_ms.min(dur)
    } else {
      target_ms
    };
    self.paused_position_ms = safe_target;

    self.current_decoder = None;

    if self.state == PlayerState::Playing {
      self.play_internal(safe_target, false)?;
    }

    Ok(())
  }

  /// 停止播放
  pub fn stop(&mut self) {
    if let Some(sig) = self.current_fade_out_signal.take() {
      sig.store(0, Ordering::Relaxed);
    }
    if let Some(prev) = self.current_source.take() {
      prev.cancel();
    }
    self.current_decoder = None;
    if let Some(sink) = self.sink.as_ref() {
      sink.stop();
    }
    self.state = PlayerState::Stopped;
    self.paused_position_ms = 0;
    self.samples_consumed.store(0, Ordering::SeqCst);
  }

  /// 设置播放音量 (0.0 ~ 1.0)
  pub fn set_volume(&mut self, volume: f32) {
    self.volume = volume.clamp(0.0, 1.0);
    if let Some(sink) = self.sink.as_ref() {
      sink.set_volume(self.volume);
    }
  }

  pub fn volume(&self) -> f32 {
    self.volume
  }

  /// 设置渐变时长 (毫秒)
  pub fn set_fade_duration_ms(&mut self, ms: u64) {
    self.fade_duration_ms = ms;
  }

  pub fn fade_duration_ms(&self) -> u64 {
    self.fade_duration_ms
  }

  /// 获取当前播放实时位置
  pub fn position_ms(&self) -> u64 {
    if self.state == PlayerState::Playing {
      let samples = self.samples_consumed.load(Ordering::Relaxed);
      let sr = self.sample_rate.load(Ordering::Relaxed).max(1) as u64;
      let ms = samples * 1000 / (sr * 2);
      let dur = self.duration_ms.load(Ordering::SeqCst);
      if dur > 0 {
        return ms.min(dur);
      }
      return ms;
    }
    self.paused_position_ms
  }

  /// 获取当前音频总时长 (毫秒)
  pub fn duration_ms(&self) -> u64 {
    self.duration_ms.load(Ordering::SeqCst)
  }

  /// 获取当前播放状态
  pub fn state(&self) -> PlayerState {
    self.state
  }

  /// 获取均衡器参数管理器
  pub fn equalizer_params(&self) -> Arc<EqualizerParams> {
    self.equalizer_params.clone()
  }

  /// 设置指定均衡器频段增益
  pub fn set_eq_gain(&mut self, band_index: usize, gain_db: f32) {
    self.equalizer_params.set_band_gain(band_index, gain_db);
  }

  /// 应用均衡器预设
  pub fn set_eq_preset(&mut self, preset_name: &str) {
    let preset = match preset_name.to_lowercase().as_str() {
      "pop" => EqPreset::Pop,
      "rock" => EqPreset::Rock,
      "classical" => EqPreset::Classical,
      "vocal" => EqPreset::Vocal,
      "bassboost" | "bass_boost" | "bass" => EqPreset::BassBoost,
      _ => EqPreset::Flat,
    };
    self.equalizer_params.apply_preset(preset);
  }

  /// 设置播放倍速
  pub fn set_speed(&mut self, speed: f32) {
    self.tempo_params.set_speed(speed);
  }

  /// 获取变速变调参数管理器
  pub fn tempo_params(&self) -> Arc<TempoParams> {
    self.tempo_params.clone()
  }

  /// 获取播放头当前位置的频谱
  pub fn get_spectrum(&self) -> [f32; SPECTRUM_BANDS] {
    self.visualizer.get_bands()
  }
}
