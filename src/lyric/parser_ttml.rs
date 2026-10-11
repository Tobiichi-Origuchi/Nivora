use super::model::{LyricLine, LyricWord};

/// 解析 TTML 时间字符串为毫秒
pub fn parse_ttml_time(val: &str) -> Option<u64> {
  let s = val.trim();
  if s.is_empty() {
    return None;
  }

  // 处理以 "s" 结尾的纯秒数
  if let Some(sec_str) = s.strip_suffix('s') {
    let sec: f64 = sec_str.trim().parse().ok()?;
    return Some((sec * 1000.0).round() as u64);
  }

  // 处理以 "ms" 结尾的纯毫秒数
  if let Some(ms_str) = s.strip_suffix("ms") {
    let ms: u64 = ms_str.trim().parse().ok()?;
    return Some(ms);
  }

  // 处理冒号分隔格式
  let parts: Vec<&str> = s.split(':').collect();
  match parts.len() {
    2 => {
      // mm:ss.fraction
      let min: u64 = parts[0].trim().parse().ok()?;
      let sec_f: f64 = parts[1].trim().parse().ok()?;
      Some(min * 60_000 + (sec_f * 1000.0).round() as u64)
    }
    3 => {
      // hh:mm:ss.fraction
      let hr: u64 = parts[0].trim().parse().ok()?;
      let min: u64 = parts[1].trim().parse().ok()?;
      let sec_f: f64 = parts[2].trim().parse().ok()?;
      Some(hr * 3_600_000 + min * 60_000 + (sec_f * 1000.0).round() as u64)
    }
    _ => None,
  }
}

/// 提取 XML 标签中的属性值
fn extract_attr<'a>(tag_content: &'a str, attr_name: &str) -> Option<&'a str> {
  // 匹配 attr_name="..." 或 :attr_name="..."
  let search_patterns = [format!("{}=", attr_name), format!(":{}=", attr_name)];

  for pattern in &search_patterns {
    if let Some(mut pos) = tag_content.find(pattern) {
      pos += pattern.len();
      let remainder = tag_content[pos..].trim_start();
      let quote = remainder.chars().next()?;
      if quote == '"' || quote == '\'' {
        let after_quote = &remainder[1..];
        if let Some(end_quote) = after_quote.find(quote) {
          return Some(&after_quote[..end_quote]);
        }
      }
    }
  }
  None
}

/// 解析 Apple Music / QQ 音乐 TTML XML 逐字歌词
pub fn parse_ttml(xml: &str) -> Vec<LyricLine> {
  let mut lines = Vec::new();
  let mut remainder = xml;

  // 扫描每个 <p ...> ... </p> 段落
  while let Some(p_start) = remainder.find("<p") {
    let after_p = &remainder[p_start..];
    let p_tag_end = match after_p.find('>') {
      Some(idx) => idx,
      None => break,
    };

    let p_open_tag = &after_p[..p_tag_end];
    let p_close = match after_p.find("</p>") {
      Some(idx) => idx,
      None => break,
    };

    let p_body = &after_p[p_tag_end + 1..p_close];

    let line_begin = extract_attr(p_open_tag, "begin")
      .and_then(parse_ttml_time)
      .unwrap_or(0);
    let line_end = extract_attr(p_open_tag, "end")
      .and_then(parse_ttml_time)
      .unwrap_or(line_begin + 3000);
    let role = extract_attr(p_open_tag, "role").unwrap_or("");
    let is_bg = role.contains("bg") || role.contains("x-bg");
    let agent = extract_attr(p_open_tag, "agent").unwrap_or("");
    let is_duet = !agent.is_empty() && agent != "v1";

    let mut words = Vec::new();
    let mut translated_lyric = None;
    let mut roman_lyric = None;

    // 扫描 <p> 内嵌的 <span ...> 词节点
    let mut span_remainder = p_body;
    while let Some(s_start) = span_remainder.find("<span") {
      let after_s = &span_remainder[s_start..];
      let s_tag_end = match after_s.find('>') {
        Some(idx) => idx,
        None => break,
      };
      let span_open_tag = &after_s[..s_tag_end];
      let s_close = match after_s.find("</span>") {
        Some(idx) => idx,
        None => break,
      };

      let span_text = &after_s[s_tag_end + 1..s_close];
      let span_role = extract_attr(span_open_tag, "role").unwrap_or("");

      if span_role.contains("translation") || span_role.contains("x-translation") {
        translated_lyric = Some(span_text.trim().to_string());
      } else if span_role.contains("roman") || span_role.contains("x-roman") {
        roman_lyric = Some(span_text.trim().to_string());
      } else {
        let w_begin = extract_attr(span_open_tag, "begin")
          .and_then(parse_ttml_time)
          .unwrap_or(line_begin);
        let w_end = extract_attr(span_open_tag, "end")
          .and_then(parse_ttml_time)
          .unwrap_or(line_end);

        words.push(LyricWord {
          start_time: w_begin,
          end_time: w_end,
          word: span_text.to_string(),
          roman_word: None,
        });
      }

      span_remainder = &after_s[s_close + 7..];
    }

    // 若没有细分 span 词标签，则整段文本视为单个词
    if words.is_empty() {
      let clean_text = clean_xml_text(p_body);
      if !clean_text.is_empty() {
        words.push(LyricWord {
          start_time: line_begin,
          end_time: line_end,
          word: clean_text,
          roman_word: None,
        });
      }
    }

    if !words.is_empty() {
      lines.push(LyricLine {
        start_time: line_begin,
        end_time: line_end,
        words,
        translated_lyric,
        roman_lyric,
        is_bg,
        is_duet,
      });
    }

    remainder = &after_p[p_close + 4..];
  }

  lines.sort_by_key(|l| l.start_time);
  lines
}

fn clean_xml_text(text: &str) -> String {
  let mut out = String::new();
  let mut in_tag = false;
  for ch in text.chars() {
    if ch == '<' {
      in_tag = true;
    } else if ch == '>' {
      in_tag = false;
    } else if !in_tag {
      out.push(ch);
    }
  }
  out.trim().to_string()
}
