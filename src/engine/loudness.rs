const OUTPUT_CEILING: f32 = 0.98;
const LIMITER_RELEASE: f32 = 0.0005;

/// 峰值限幅器与削波保护器
pub struct OutputLimiter {
  gain: f32,
}

impl Default for OutputLimiter {
  fn default() -> Self {
    Self::new()
  }
}

impl OutputLimiter {
  pub fn new() -> Self {
    Self { gain: 1.0 }
  }

  /// 对立体声采样点流执行动态增益限幅
  pub fn process(&mut self, samples: &mut [f32]) {
    for sample in samples {
      let peak = sample.abs();
      let target_gain = if peak > OUTPUT_CEILING {
        OUTPUT_CEILING / peak
      } else {
        1.0
      };

      if peak * self.gain >= OUTPUT_CEILING {
        self.gain = target_gain;
      } else {
        self.gain += (1.0 - self.gain) * LIMITER_RELEASE;
      }

      *sample *= self.gain;
      *sample = sample.clamp(-OUTPUT_CEILING, OUTPUT_CEILING);
    }
  }
}
