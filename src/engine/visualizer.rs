use parking_lot::Mutex;
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::f32::consts::PI;
use std::sync::Arc;

pub const SPECTRUM_BANDS: usize = 32;
pub const FFT_SIZE: usize = 1024;

/// 音频频谱提取器
pub struct FftVisualizer {
  fft: Arc<dyn Fft<f32>>,
  window: Vec<f32>,
  buffer: Mutex<Vec<f32>>,
  decay_levels: Mutex<[f32; SPECTRUM_BANDS]>,
}

impl Default for FftVisualizer {
  fn default() -> Self {
    Self::new()
  }
}

impl FftVisualizer {
  pub fn new() -> Self {
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);

    // 构建汉宁窗
    let window = (0..FFT_SIZE)
      .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (FFT_SIZE - 1) as f32).cos()))
      .collect();

    Self {
      fft,
      window,
      buffer: Mutex::new(Vec::with_capacity(FFT_SIZE * 2)),
      decay_levels: Mutex::new([0.0; SPECTRUM_BANDS]),
    }
  }

  /// 送入单声道/立体声采样点数据
  pub fn push_samples(&self, samples: &[f32], channels: usize) {
    if let Some(mut buf) = self.buffer.try_lock() {
      for frame in samples.chunks(channels) {
        let mono = if channels > 1 {
          (frame[0] + frame[1]) * 0.5
        } else {
          frame[0]
        };
        buf.push(mono);
      }

      // 保持缓冲区大小在合理范围
      if buf.len() > FFT_SIZE * 4 {
        let excess = buf.len() - FFT_SIZE * 2;
        buf.drain(0..excess);
      }
    }
  }

  /// 获取 32 频段能量强度柱 (0.0 ~ 1.0)
  pub fn get_bands(&self) -> [f32; SPECTRUM_BANDS] {
    let buf = self.buffer.lock();
    if buf.len() < FFT_SIZE {
      return *self.decay_levels.lock();
    }

    // 取最近的 FFT_SIZE 个样本并加窗
    let start = buf.len() - FFT_SIZE;
    let mut complex_buf: Vec<Complex<f32>> = buf[start..]
      .iter()
      .zip(&self.window)
      .map(|(&s, &w)| Complex::new(s * w, 0.0))
      .collect();

    drop(buf);

    self.fft.process(&mut complex_buf);

    // 计算各频段能量模长
    let half_size = FFT_SIZE / 2;
    let mut raw_bands = [0.0f32; SPECTRUM_BANDS];

    for (i, band) in raw_bands.iter_mut().enumerate() {
      // 对数分布频段索引
      let start_idx = ((i as f32 / SPECTRUM_BANDS as f32).powi(2) * half_size as f32) as usize;
      let end_idx = (((i + 1) as f32 / SPECTRUM_BANDS as f32).powi(2) * half_size as f32) as usize;
      let end_idx = end_idx.clamp(start_idx + 1, half_size);

      let mut sum_mag = 0.0;
      let count = (end_idx - start_idx) as f32;
      for item in &complex_buf[start_idx..end_idx] {
        sum_mag += item.norm();
      }

      let avg_mag = sum_mag / count;
      // 转换为对数感官刻度并归一化至 0.0 ~ 1.0
      let db = 20.0 * (avg_mag / (FFT_SIZE as f32 * 0.5) + 1e-6).log10();
      let norm = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
      *band = norm;
    }

    // 施加平滑下落衰减
    let mut decay = self.decay_levels.lock();
    for (i, &band) in raw_bands.iter().enumerate() {
      if band > decay[i] {
        decay[i] = band;
      } else {
        decay[i] = (decay[i] - 0.04).max(0.0);
      }
    }

    *decay
  }
}

static VISUALIZER_IN_FLIGHT: std::sync::atomic::AtomicBool =
  std::sync::atomic::AtomicBool::new(false);
static LATEST_SPECTRUM: parking_lot::RwLock<[f32; SPECTRUM_BANDS]> =
  parking_lot::RwLock::new([0.0; SPECTRUM_BANDS]);

/// 更新最新瞬态频谱数据 (返回是否成功置位在飞状态)
pub fn update_spectrum(spectrum: [f32; SPECTRUM_BANDS]) -> bool {
  *LATEST_SPECTRUM.write() = spectrum;
  VISUALIZER_IN_FLIGHT
    .compare_exchange(
      false,
      true,
      std::sync::atomic::Ordering::AcqRel,
      std::sync::atomic::Ordering::Relaxed,
    )
    .is_ok()
}

/// 读取当前频谱数据并清除在飞状态
pub fn take_spectrum() -> [f32; SPECTRUM_BANDS] {
  VISUALIZER_IN_FLIGHT.store(false, std::sync::atomic::Ordering::Release);
  *LATEST_SPECTRUM.read()
}

/// 重置频谱状态与在飞标志位
pub fn reset_spectrum() {
  VISUALIZER_IN_FLIGHT.store(false, std::sync::atomic::Ordering::Release);
  *LATEST_SPECTRUM.write() = [0.0; SPECTRUM_BANDS];
}
