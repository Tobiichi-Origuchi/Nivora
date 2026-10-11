use std::f32::consts::PI;

pub const EQ_BAND_FREQS: [f32; 10] = [
  31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// 单个二阶双二阶 IIR 滤波节 (Biquad Filter)
#[derive(Clone, Debug, Default)]
struct Biquad {
  b0: f32,
  b1: f32,
  b2: f32,
  a1: f32,
  a2: f32,
  // 左右两声道的历史状态
  x1: [f32; 2],
  x2: [f32; 2],
  y1: [f32; 2],
  y2: [f32; 2],
}

impl Biquad {
  fn set_peaking_eq(&mut self, freq: f32, sample_rate: f32, gain_db: f32, q: f32) {
    if gain_db.abs() < 0.05 {
      // 近似无增益时透传
      self.b0 = 1.0;
      self.b1 = 0.0;
      self.b2 = 0.0;
      self.a1 = 0.0;
      self.a2 = 0.0;
      return;
    }

    let a = 10.0f32.powf(gain_db / 40.0);
    let omega = 2.0 * PI * (freq / sample_rate).clamp(0.001, 0.49);
    let alpha = omega.sin() / (2.0 * q);
    let cos_w = omega.cos();

    let a0 = 1.0 + alpha / a;
    self.b0 = (1.0 + alpha * a) / a0;
    self.b1 = (-2.0 * cos_w) / a0;
    self.b2 = (1.0 - alpha * a) / a0;
    self.a1 = (-2.0 * cos_w) / a0;
    self.a2 = (1.0 - alpha / a) / a0;
  }

  #[inline]
  fn process(&mut self, ch: usize, input: f32) -> f32 {
    let output = self.b0 * input + self.b1 * self.x1[ch] + self.b2 * self.x2[ch]
      - self.a1 * self.y1[ch]
      - self.a2 * self.y2[ch];

    self.x2[ch] = self.x1[ch];
    self.x1[ch] = input;
    self.y2[ch] = self.y1[ch];
    self.y1[ch] = output;

    output
  }

  fn reset_state(&mut self) {
    self.x1 = [0.0; 2];
    self.x2 = [0.0; 2];
    self.y1 = [0.0; 2];
    self.y2 = [0.0; 2];
  }
}

/// 均衡器预设
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqPreset {
  Flat,
  Pop,
  Rock,
  Classical,
  Vocal,
  BassBoost,
}

/// 均衡器
#[derive(Clone, Debug)]
pub struct Equalizer {
  enabled: bool,
  sample_rate: f32,
  gains: [f32; 10],
  filters: Vec<Biquad>,
}

impl Default for Equalizer {
  fn default() -> Self {
    Self::new(44_100.0)
  }
}

impl Equalizer {
  pub fn new(sample_rate: f32) -> Self {
    let mut eq = Self {
      enabled: true,
      sample_rate,
      gains: [0.0; 10],
      filters: vec![Biquad::default(); 10],
    };
    eq.recalculate();
    eq
  }

  pub fn set_enabled(&mut self, enabled: bool) {
    self.enabled = enabled;
    if !enabled {
      for f in &mut self.filters {
        f.reset_state();
      }
    }
  }

  pub fn is_enabled(&self) -> bool {
    self.enabled
  }

  pub fn set_sample_rate(&mut self, sample_rate: f32) {
    self.sample_rate = sample_rate;
    self.recalculate();
  }

  /// 设置某个频段的增益 (-12.0 dB ~ +12.0 dB)
  pub fn set_band_gain(&mut self, band_index: usize, gain_db: f32) {
    if band_index < 10 {
      self.gains[band_index] = gain_db.clamp(-12.0, 12.0);
      self.filters[band_index].set_peaking_eq(
        EQ_BAND_FREQS[band_index],
        self.sample_rate,
        self.gains[band_index],
        1.414,
      );
    }
  }

  /// 获取当前 10 频段增益数组引用
  pub fn gains(&self) -> &[f32; 10] {
    &self.gains
  }

  /// 重置所有频段增益为 0.0 dB
  pub fn reset(&mut self) {
    self.apply_preset(EqPreset::Flat);
  }

  /// 应用预设
  pub fn apply_preset(&mut self, preset: EqPreset) {
    let gains = match preset {
      EqPreset::Flat => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
      EqPreset::Pop => [1.5, 2.0, 3.0, 1.0, 0.0, 0.5, 1.5, 2.0, 2.5, 1.5],
      EqPreset::Rock => [4.0, 3.0, 1.5, 0.0, -1.0, 0.5, 2.0, 3.5, 4.0, 4.0],
      EqPreset::Classical => [3.0, 2.5, 2.0, 1.0, -0.5, 0.0, 1.0, 2.0, 2.5, 3.0],
      EqPreset::Vocal => [-2.0, -1.0, 0.0, 2.0, 3.5, 4.0, 3.0, 1.5, 0.5, -0.5],
      EqPreset::BassBoost => [6.0, 5.0, 3.5, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    };
    self.gains = gains;
    self.recalculate();
  }

  fn recalculate(&mut self) {
    for (i, &freq) in EQ_BAND_FREQS.iter().enumerate() {
      self.filters[i].set_peaking_eq(freq, self.sample_rate, self.gains[i], 1.414);
    }
  }

  /// 原地处理立体声 PCM 采样点
  pub fn process(&mut self, samples: &mut [f32]) {
    if !self.enabled {
      return;
    }

    for chunk in samples.chunks_mut(2) {
      let left = chunk[0];
      let right = chunk[1];

      let mut out_l = left;
      let mut out_r = right;

      for filter in &mut self.filters {
        out_l = filter.process(0, out_l);
        out_r = filter.process(1, out_r);
      }

      chunk[0] = out_l;
      chunk[1] = out_r;
    }
  }

  /// 与无锁参数快照进行同步并按需热重算滤波系数
  pub fn sync_with_params(&mut self, params: &EqualizerParams) {
    let enabled = params.is_enabled();
    if self.enabled != enabled {
      self.set_enabled(enabled);
    }
    let current_gains = params.load_gains();
    if self.gains != current_gains {
      self.gains = current_gains;
      self.recalculate();
    }
  }
}

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// 无锁均衡器参数管理器
pub struct EqualizerParams {
  enabled: AtomicBool,
  gains: [AtomicU32; 10],
}

impl Default for EqualizerParams {
  fn default() -> Self {
    Self::new()
  }
}

impl EqualizerParams {
  pub fn new() -> Self {
    Self {
      enabled: AtomicBool::new(true),
      gains: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
    }
  }

  pub fn set_enabled(&self, enabled: bool) {
    self.enabled.store(enabled, Ordering::Relaxed);
  }

  pub fn is_enabled(&self) -> bool {
    self.enabled.load(Ordering::Relaxed)
  }

  pub fn set_band_gain(&self, band_index: usize, gain_db: f32) {
    if band_index < 10 {
      let clamped = gain_db.clamp(-12.0, 12.0);
      self.gains[band_index].store(clamped.to_bits(), Ordering::Relaxed);
    }
  }

  pub fn get_band_gain(&self, band_index: usize) -> f32 {
    if band_index < 10 {
      f32::from_bits(self.gains[band_index].load(Ordering::Relaxed))
    } else {
      0.0
    }
  }

  pub fn load_gains(&self) -> [f32; 10] {
    std::array::from_fn(|i| f32::from_bits(self.gains[i].load(Ordering::Relaxed)))
  }

  pub fn apply_preset(&self, preset: EqPreset) {
    let gains: [f32; 10] = match preset {
      EqPreset::Flat => [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
      EqPreset::Pop => [1.5, 2.0, 3.0, 1.0, 0.0, 0.5, 1.5, 2.0, 2.5, 1.5],
      EqPreset::Rock => [4.0, 3.0, 1.5, 0.0, -1.0, 0.5, 2.0, 3.5, 4.0, 4.0],
      EqPreset::Classical => [3.0, 2.5, 2.0, 1.0, -0.5, 0.0, 1.0, 2.0, 2.5, 3.0],
      EqPreset::Vocal => [-2.0, -1.0, 0.0, 2.0, 3.5, 4.0, 3.0, 1.5, 0.5, -0.5],
      EqPreset::BassBoost => [6.0, 5.0, 3.5, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    };
    for (i, &g) in gains.iter().enumerate() {
      self.gains[i].store(g.to_bits(), Ordering::Relaxed);
    }
  }
}
