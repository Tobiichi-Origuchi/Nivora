use serde::{Deserialize, Serialize};

/// 歌词基础时间片段
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LyricSpan {
  pub start_time: u64,
  pub end_time: u64,
  pub text: String,
}

/// 逐字歌词音节/单词
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LyricWord {
  pub start_time: u64,
  pub end_time: u64,
  pub word: String,
  pub roman_word: Option<String>,
}

impl LyricWord {
  pub fn duration_ms(&self) -> u64 {
    self.end_time.saturating_sub(self.start_time)
  }

  /// 计算在当前毫秒时间下的发音进度 (0.0 ~ 1.0)
  pub fn progress(&self, current_time: u64) -> f32 {
    if current_time < self.start_time {
      0.0
    } else if current_time >= self.end_time {
      1.0
    } else {
      let dur = self.duration_ms();
      if dur == 0 {
        1.0
      } else {
        (current_time - self.start_time) as f32 / dur as f32
      }
    }
  }
}

/// 歌词行实体
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LyricLine {
  pub start_time: u64,
  pub end_time: u64,
  pub words: Vec<LyricWord>,
  pub translated_lyric: Option<String>,
  pub roman_lyric: Option<String>,
  pub is_bg: bool,
  pub is_duet: bool,
}

impl LyricLine {
  /// 获取当前行的主歌词纯文本
  pub fn text(&self) -> String {
    if self.words.is_empty() {
      String::new()
    } else {
      self
        .words
        .iter()
        .map(|w| w.word.as_str())
        .collect::<String>()
    }
  }

  pub fn duration_ms(&self) -> u64 {
    self.end_time.saturating_sub(self.start_time)
  }

  /// 计算在当前毫秒时间下整行的累计点亮进度
  pub fn line_progress(&self, current_time: u64) -> f32 {
    if current_time < self.start_time {
      return 0.0;
    }
    if current_time >= self.end_time {
      return 1.0;
    }

    if self.words.is_empty() {
      let dur = self.duration_ms();
      if dur == 0 {
        1.0
      } else {
        ((current_time - self.start_time) as f32 / dur as f32).clamp(0.0, 1.0)
      }
    } else {
      let total_chars: usize = self
        .words
        .iter()
        .map(|w| w.word.chars().count().max(1))
        .sum();
      if total_chars == 0 {
        return 1.0;
      }

      let mut passed_chars = 0.0f32;
      for word in &self.words {
        let char_count = word.word.chars().count().max(1) as f32;
        if current_time < word.start_time {
          break;
        } else if current_time >= word.end_time {
          passed_chars += char_count;
        } else {
          let w_progress = word.progress(current_time);
          passed_chars += char_count * w_progress;
          break;
        }
      }
      (passed_chars / total_chars as f32).clamp(0.0, 1.0)
    }
  }

  /// 查找当前时间下正在发音的单词索引及该词的进度
  pub fn active_word_index_and_progress(&self, current_time: u64) -> (Option<usize>, f32) {
    if self.words.is_empty() {
      return (None, 0.0);
    }
    for (i, word) in self.words.iter().enumerate() {
      if current_time >= word.start_time && current_time < word.end_time {
        return (Some(i), word.progress(current_time));
      }
    }
    if current_time >= self.end_time {
      (Some(self.words.len() - 1), 1.0)
    } else if current_time < self.start_time {
      (Some(0), 0.0)
    } else {
      (None, 0.0)
    }
  }
}

/// 间奏倒计时状态
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InterludeState {
  pub active: bool,
  pub start_time: u64,
  pub end_time: u64,
  pub remaining_ms: u64,
  /// 倒计时点亮圆点数量 (0, 1, 2, 3)
  pub dots: u8,
}
