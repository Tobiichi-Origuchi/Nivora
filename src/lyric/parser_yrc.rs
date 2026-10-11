use super::model::{LyricLine, LyricWord};

/// 解析网易云逐字 YRC 格式
pub fn parse_yrc(text: &str) -> Vec<LyricLine> {
  let mut lines = Vec::new();

  for raw_line in text.lines() {
    let trimmed = raw_line.trim();
    if trimmed.is_empty() {
      continue;
    }

    // 1. 解析行头 [line_start, line_dur]
    if !trimmed.starts_with('[') {
      continue;
    }
    let header_end = match trimmed.find(']') {
      Some(idx) => idx,
      None => continue,
    };

    let header_content = &trimmed[1..header_end];
    let (line_start, line_dur) = match parse_header_durations(header_content) {
      Some(pair) => pair,
      None => continue,
    };

    let rest = &trimmed[header_end + 1..];
    let mut words = Vec::new();

    // 2. 逐词解析 (start, dur, 0)word_text
    let mut curr_offset = 0;

    while let Some((_tag_start, tag_end, w_start, w_dur)) = find_next_tag(&rest[curr_offset..]) {
      let abs_tag_end = curr_offset + tag_end;
      let text_start = abs_tag_end + 1;

      // 查找下一个标签的起始位置作为当前词文本的结束点
      let text_end = if let Some((next_tag_start, _, _, _)) = find_next_tag(&rest[text_start..]) {
        text_start + next_tag_start
      } else {
        rest.len()
      };

      let word_str = &rest[text_start..text_end];
      words.push(LyricWord {
        start_time: w_start,
        end_time: w_start + w_dur,
        word: word_str.to_string(),
        roman_word: None,
      });

      curr_offset = text_end;
    }

    if words.is_empty() {
      continue;
    }

    // 3. 和声/背景音判定
    let full_text: String = words.iter().map(|w| w.word.as_str()).collect();
    let is_bg = is_background_line(&full_text);

    lines.push(LyricLine {
      start_time: line_start,
      end_time: line_start + line_dur,
      words,
      translated_lyric: None,
      roman_lyric: None,
      is_bg,
      is_duet: false,
    });
  }

  lines.sort_by_key(|l| l.start_time);
  lines
}

fn parse_header_durations(content: &str) -> Option<(u64, u64)> {
  let mut parts = content.split(',');
  let start: u64 = parts.next()?.trim().parse().ok()?;
  let dur: u64 = parts.next()?.trim().parse().ok()?;
  Some((start, dur))
}

fn parse_word_tag(tag: &str) -> Option<(u64, u64)> {
  let mut parts = tag.split(',');
  let start: u64 = parts.next()?.trim().parse().ok()?;
  let dur: u64 = parts.next()?.trim().parse().ok()?;
  let _flag: u64 = parts.next()?.trim().parse().ok()?;
  Some((start, dur))
}

/// 扫描字符串中下一个有效的 (start,dur,flag) 词时间戳标签
fn find_next_tag(s: &str) -> Option<(usize, usize, u64, u64)> {
  let mut search_from = 0;
  while let Some(open) = s[search_from..].find('(') {
    let abs_open = search_from + open;
    if let Some(close) = s[abs_open..].find(')') {
      let abs_close = abs_open + close;
      let tag_content = &s[abs_open + 1..abs_close];
      if let Some((start, dur)) = parse_word_tag(tag_content) {
        return Some((abs_open, abs_close, start, dur));
      }
    }
    search_from = abs_open + 1;
  }
  None
}

fn is_background_line(text: &str) -> bool {
  let t = text.trim();
  (t.starts_with('(') && t.ends_with(')'))
    || (t.starts_with('（') && t.ends_with('）'))
    || (t.ends_with(')') && t.contains('('))
    || (t.ends_with('）') && t.contains('（'))
}
