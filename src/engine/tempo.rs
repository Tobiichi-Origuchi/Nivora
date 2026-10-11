use signalsmith_stretch::Stretch;

/// 变速变调音频处理器
pub struct TempoProcessor {
  speed: f32,
  pitch_semitones: f32,
  channels: usize,
  sample_rate: u32,
  stretch: Option<Stretch>,
  scratch_buf: Vec<f32>,
}

impl Default for TempoProcessor {
  fn default() -> Self {
    Self {
      speed: 1.0,
      pitch_semitones: 0.0,
      channels: 2,
      sample_rate: 44100,
      stretch: None,
      scratch_buf: Vec::new(),
    }
  }
}

impl TempoProcessor {
  pub fn new() -> Self {
    Self::default()
  }

  /// 设置播放倍速 (0.5x ~ 2.0x)
  pub fn set_speed(&mut self, speed: f32) {
    self.speed = speed.clamp(0.5, 2.0);
  }

  pub fn speed(&self) -> f32 {
    self.speed
  }

  /// 设置变调半音数 (-12.0 ~ +12.0 半音，0.0 为原调)
  pub fn set_pitch_semitones(&mut self, semitones: f32) {
    self.pitch_semitones = semitones.clamp(-12.0, 12.0);
    if let Some(ref mut stretch) = self.stretch {
      stretch.set_transpose_factor_semitones(self.pitch_semitones, None);
    }
  }

  pub fn pitch_semitones(&self) -> f32 {
    self.pitch_semitones
  }

  /// 重新配置通道数与采样率
  pub fn configure(&mut self, channels: usize, sample_rate: u32) {
    self.channels = channels;
    self.sample_rate = sample_rate;
    let mut stretch = Stretch::preset_default(channels as u32, sample_rate);
    stretch.set_transpose_factor_semitones(self.pitch_semitones, None);
    self.stretch = Some(stretch);
  }

  /// 处理交织 PCM 音频采样
  pub fn process_interleaved(
    &mut self,
    input: &[f32],
    channels: usize,
    sample_rate: u32,
  ) -> &[f32] {
    if input.is_empty() || channels == 0 {
      return &[];
    }

    // 若原速且原调，直接快速拷贝至内部缓冲并返回引用
    if (self.speed - 1.0).abs() < 0.005 && self.pitch_semitones.abs() < 0.01 {
      self.scratch_buf.clear();
      self.scratch_buf.extend_from_slice(input);
      return &self.scratch_buf;
    }

    if self.stretch.is_none() || self.channels != channels || self.sample_rate != sample_rate {
      self.configure(channels, sample_rate);
    }

    let stretch = self.stretch.as_mut().expect("Stretch 算法实例已初始化");
    stretch.set_transpose_factor_semitones(self.pitch_semitones, None);

    let in_frames = input.len() / channels;
    let out_frames = ((in_frames as f64 / self.speed as f64).round() as usize).max(1);
    let out_len = out_frames * channels;

    self.scratch_buf.resize(out_len, 0.0);
    self.scratch_buf.fill(0.0);

    stretch.process(input, &mut self.scratch_buf);
    &self.scratch_buf
  }

  /// 与参数快照进行同步
  pub fn sync_with_params(&mut self, params: &TempoParams) {
    let s = params.speed();
    let p = params.pitch_semitones();
    if (self.speed - s).abs() > 0.001 {
      self.set_speed(s);
    }
    if (self.pitch_semitones - p).abs() > 0.001 {
      self.set_pitch_semitones(p);
    }
  }
}

use std::sync::atomic::{AtomicU32, Ordering};

/// 变速变调参数管理器
pub struct TempoParams {
  speed: AtomicU32,
  pitch: AtomicU32,
}

impl Default for TempoParams {
  fn default() -> Self {
    Self::new()
  }
}

impl TempoParams {
  pub fn new() -> Self {
    Self {
      speed: AtomicU32::new(1.0f32.to_bits()),
      pitch: AtomicU32::new(0.0f32.to_bits()),
    }
  }

  pub fn set_speed(&self, speed: f32) {
    let clamped = speed.clamp(0.5, 2.0);
    self.speed.store(clamped.to_bits(), Ordering::Relaxed);
  }

  pub fn speed(&self) -> f32 {
    f32::from_bits(self.speed.load(Ordering::Relaxed))
  }

  pub fn set_pitch_semitones(&self, semitones: f32) {
    let clamped = semitones.clamp(-12.0, 12.0);
    self.pitch.store(clamped.to_bits(), Ordering::Relaxed);
  }

  pub fn pitch_semitones(&self) -> f32 {
    f32::from_bits(self.pitch.load(Ordering::Relaxed))
  }
}
