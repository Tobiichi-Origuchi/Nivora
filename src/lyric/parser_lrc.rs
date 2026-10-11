use super::model::{LyricLine, LyricWord};

/// 解析分、秒、毫秒为绝对毫秒
fn parse_time_stamp(min_s: &str, sec_s: &str, ms_s: &str) -> Option<u64> {
  let min: u64 = min_s.parse().ok()?;
  let sec: u64 = sec_s.parse().ok()?;
  let mut ms: u64 = ms_s.parse().ok()?;

  // 归一化毫秒
  if ms_s.len() == 1 {
    ms *= 100;
  } else if ms_s.len() == 2 {
    ms *= 10;
  }

  Some(min * 60_000 + sec * 1_000 + ms)
}

/// 解析单时间戳或行首多时间戳
fn extract_timestamps_and_text(line: &str) -> (Vec<u64>, &str) {
  let mut timestamps = Vec::new();
  let mut remainder = line;

  while let Some(start_idx) = remainder.find('[') {
    if start_idx != 0 {
      break;
    }
    if let Some(end_idx) = remainder.find(']') {
      let tag = &remainder[1..end_idx];
      // 检查格式 mm:ss.xx 或 mm:ss:xx
      if let Some((min_part, sec_ms_part)) = tag.split_once(':')
        && let Some((sec_part, ms_part)) = sec_ms_part
          .split_once('.')
          .or_else(|| sec_ms_part.split_once(':'))
        && let Some(t_ms) = parse_time_stamp(min_part, sec_part, ms_part)
      {
        timestamps.push(t_ms);
      }
      remainder = &remainder[end_idx + 1..];
    } else {
      break;
    }
  }

  (timestamps, remainder.trim())
}

/// 检查是否为和声/背景音
fn is_background_text(text: &str) -> bool {
  let trimmed = text.trim();
  (trimmed.starts_with('(') && trimmed.ends_with(')'))
    || (trimmed.starts_with('（') && trimmed.ends_with('）'))
    || (trimmed.ends_with(')') && trimmed.contains('('))
    || (trimmed.ends_with('）') && trimmed.contains('（'))
}

/// 尝试解析 ESLRC 尖括号逐字标签
fn parse_eslrc_words(text: &str) -> Option<Vec<LyricWord>> {
  if !text.contains('<') {
    return None;
  }

  let mut words = Vec::new();
  let mut remainder = text;

  while let Some(open) = remainder.find('<') {
    if let Some(close) = remainder[open..].find('>') {
      let close_pos = open + close;
      let tag = &remainder[open + 1..close_pos];
      let after_tag = &remainder[close_pos + 1..];
      let next_open = after_tag.find('<').unwrap_or(after_tag.len());
      let word_text = &after_tag[..next_open];

      if let Some((min_part, sec_ms_part)) = tag.split_once(':') {
        let sec_ms = sec_ms_part
          .split_once('.')
          .or_else(|| sec_ms_part.split_once(':'));
        if let Some((sec_part, ms_part)) = sec_ms
          && let Some(start_time) = parse_time_stamp(min_part, sec_part, ms_part)
        {
          words.push(LyricWord {
            start_time,
            end_time: start_time + 400, // 初始预估，后续连接时重写
            word: word_text.to_string(),
            roman_word: None,
          });
        }
      }
      remainder = &after_tag[next_open..];
    } else {
      break;
    }
  }

  if words.is_empty() {
    return None;
  }

  // 链接相邻字的结束时间
  for i in 0..words.len().saturating_sub(1) {
    let next_start = words[i + 1].start_time;
    if next_start >= words[i].start_time {
      words[i].end_time = next_start;
    }
  }

  Some(words)
}

/// 解析标准 LRC 文本
pub fn parse_lrc(text: &str) -> Vec<LyricLine> {
  let mut raw_lines = Vec::new();

  for line in text.lines() {
    let trimmed = line.trim();
    if trimmed.is_empty() {
      continue;
    }

    // 跳过 ID3/LRC 标签元数据
    if trimmed.starts_with("[ti:")
      || trimmed.starts_with("[ar:")
      || trimmed.starts_with("[al:")
      || trimmed.starts_with("[by:")
      || trimmed.starts_with("[offset:")
      || trimmed.starts_with('{')
    {
      continue;
    }

    let (timestamps, content) = extract_timestamps_and_text(trimmed);
    if timestamps.is_empty() || content.is_empty() {
      continue;
    }

    let is_bg = is_background_text(content);
    let parsed_words = parse_eslrc_words(content);

    // 处理一行中含有多个时间戳的复用情况
    for start_time in timestamps {
      let words = match &parsed_words {
        Some(w) => w.clone(),
        None => vec![LyricWord {
          start_time,
          end_time: start_time + 3_000,
          word: content.to_string(),
          roman_word: None,
        }],
      };

      raw_lines.push(LyricLine {
        start_time,
        end_time: start_time + 3_000,
        words,
        translated_lyric: None,
        roman_lyric: None,
        is_bg,
        is_duet: false,
      });
    }
  }

  // 按起始时间排序
  raw_lines.sort_by_key(|l| l.start_time);

  // 动态调整每行的结束时间
  let len = raw_lines.len();
  for i in 0..len {
    let current_start = raw_lines[i].start_time;
    let next_start = if i + 1 < len {
      raw_lines[i + 1].start_time
    } else {
      current_start + 4_000
    };

    let max_line_dur = 8_000;
    let end_time = if next_start > current_start {
      (current_start + max_line_dur).min(next_start)
    } else {
      current_start + 3_000
    };

    raw_lines[i].end_time = end_time;

    // 若该行仅含单个整体词，则同步更新该词的结束时间
    if raw_lines[i].words.len() == 1 {
      raw_lines[i].words[0].end_time = end_time;
    }
  }

  raw_lines
}
