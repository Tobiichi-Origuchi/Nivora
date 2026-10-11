use super::model::{InterludeState, LyricLine};
use parking_lot::RwLock;

const MIN_INTERLUDE_GAP_MS: u64 = 5_000;

/// 播放进度时间轴追踪器
pub struct LyricTimeline {
  lines: Vec<LyricLine>,
  last_index: RwLock<Option<usize>>,
  offset_ms: RwLock<i64>,
}

impl LyricTimeline {
  pub fn new(mut lines: Vec<LyricLine>) -> Self {
    lines.sort_by_key(|l| l.start_time);
    Self {
      lines,
      last_index: RwLock::new(None),
      offset_ms: RwLock::new(0),
    }
  }

  pub fn lines(&self) -> &[LyricLine] {
    &self.lines
  }

  pub fn is_empty(&self) -> bool {
    self.lines.is_empty()
  }

  pub fn len(&self) -> usize {
    self.lines.len()
  }

  pub fn offset_ms(&self) -> i64 {
    *self.offset_ms.read()
  }

  pub fn set_offset_ms(&self, offset_ms: i64) {
    *self.offset_ms.write() = offset_ms;
    *self.last_index.write() = None;
  }

  /// 根据当前播放时间定位激活的歌词行索引
  pub fn find_active_index(&self, current_time: u64) -> Option<usize> {
    let current_time = (current_time as i64 + *self.offset_ms.read()).max(0) as u64;
    if self.lines.is_empty() {
      return None;
    }

    // 1. 快速缓存路径：检查上一帧索引或下一行
    {
      let last_opt = *self.last_index.read();
      if let Some(prev) = last_opt
        && prev < self.lines.len()
      {
        let cur = &self.lines[prev];
        if current_time >= cur.start_time && current_time < cur.end_time {
          return Some(prev);
        }
        if prev + 1 < self.lines.len() {
          let next = &self.lines[prev + 1];
          if current_time >= next.start_time && current_time < next.end_time {
            *self.last_index.write() = Some(prev + 1);
            return Some(prev + 1);
          }
        }
      }
    }

    // 2. 二分查找：找到最后一个 start_time <= current_time 的行
    let mut lo = 0;
    let mut hi = self.lines.len() as isize - 1;
    let mut best: Option<usize> = None;

    while lo <= hi {
      let mid = ((lo + hi) / 2) as usize;
      if self.lines[mid].start_time <= current_time {
        best = Some(mid);
        lo = mid as isize + 1;
      } else {
        hi = mid as isize - 1;
      }
    }

    // 若命中行处于该行时间范围内，或处于该行 end 与下一行 start 之间的过渡期，均停留在此行
    let matched = if let Some(idx) = best {
      if current_time < self.lines[idx].end_time {
        Some(idx)
      } else if idx + 1 < self.lines.len() {
        if current_time < self.lines[idx + 1].start_time {
          Some(idx)
        } else {
          Some(idx + 1)
        }
      } else {
        Some(idx)
      }
    } else {
      // 在第一行之前的空白前奏
      None
    };

    *self.last_index.write() = matched;
    matched
  }

  /// 获取指定行当前的发音进度与激活单词进度 (0.0 .. 1.0)
  pub fn word_progress(&self, line_index: usize, current_time: u64) -> f32 {
    let current_time = (current_time as i64 + *self.offset_ms.read()).max(0) as u64;
    if let Some(line) = self.lines.get(line_index) {
      let (_, progress) = line.active_word_index_and_progress(current_time);
      progress
    } else {
      0.0
    }
  }

  /// 获取指定行当前整行音节加权高精度发音累计进度 (0.0 .. 1.0)
  pub fn line_progress(&self, line_index: usize, current_time: u64) -> f32 {
    let current_time = (current_time as i64 + *self.offset_ms.read()).max(0) as u64;
    if let Some(line) = self.lines.get(line_index) {
      line.line_progress(current_time)
    } else {
      0.0
    }
  }

  /// 检测当前时间是否处于歌曲长间奏或前奏中，并计算呼吸倒计时圆点
  pub fn detect_interlude(&self, current_time: u64) -> InterludeState {
    let current_time = (current_time as i64 + *self.offset_ms.read()).max(0) as u64;
    if self.lines.is_empty() {
      return InterludeState::default();
    }

    // 1. 检查歌曲开篇前奏 (起始到第一行歌词时间间隔 >= 5 秒)
    let first_start = self.lines[0].start_time;
    if current_time < first_start && first_start >= MIN_INTERLUDE_GAP_MS {
      let total_prelude = first_start;
      let remaining = first_start.saturating_sub(current_time);
      let progress = current_time as f64 / total_prelude.max(1) as f64;
      let dots = calc_countdown_dots(progress);

      return InterludeState {
        active: true,
        start_time: 0,
        end_time: first_start,
        remaining_ms: remaining,
        dots,
      };
    }

    // 2. 检查段落间奏 (当前行结束时间与下一行开始时间间隔 >= 5 秒)
    if let Some(active_idx) = self.find_active_index(current_time) {
      let current_line = &self.lines[active_idx];
      if current_time >= current_line.end_time && active_idx + 1 < self.lines.len() {
        let next_start = self.lines[active_idx + 1].start_time;
        let gap = next_start.saturating_sub(current_line.end_time);

        if gap >= MIN_INTERLUDE_GAP_MS && current_time < next_start {
          let elapsed = current_time.saturating_sub(current_line.end_time);
          let progress = elapsed as f64 / gap.max(1) as f64;
          let remaining = next_start.saturating_sub(current_time);
          let dots = calc_countdown_dots(progress);

          return InterludeState {
            active: true,
            start_time: current_line.end_time,
            end_time: next_start,
            remaining_ms: remaining,
            dots,
          };
        }
      }
    }

    InterludeState::default()
  }
}

/// 根据间奏进行百分比计算剩余倒计时点数 (3, 2, 1)
fn calc_countdown_dots(progress: f64) -> u8 {
  let p = progress.clamp(0.0, 1.0);
  if p < 0.33 {
    3
  } else if p < 0.66 {
    2
  } else {
    1
  }
}
