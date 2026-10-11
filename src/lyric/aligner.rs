use super::model::LyricLine;

const ALIGN_TOLERANCE_MS: i64 = 600;

/// 是否为有实际意义的翻译或音译文本
fn is_meaningful_subline(text: &str) -> bool {
  let t = text.trim();
  !t.is_empty()
    && t != "//"
    && !t.contains("作品的著作权由原作者所有")
    && !t.contains("此歌词为无版权或未经授权")
}

/// 将翻译歌词和罗马音歌词根据时间戳对齐合入主歌词行中
pub fn align_lyrics(
  mut main_lines: Vec<LyricLine>,
  translation_lines: Option<Vec<LyricLine>>,
  roman_lines: Option<Vec<LyricLine>>,
) -> Vec<LyricLine> {
  if main_lines.is_empty() {
    return main_lines;
  }

  // 确保主歌词按起始时间排序
  main_lines.sort_by_key(|l| l.start_time);

  // 1. 合并翻译歌词
  if let Some(mut trans) = translation_lines {
    trans.sort_by_key(|l| l.start_time);
    let mut i = 0;
    let mut j = 0;
    while i < main_lines.len() && j < trans.len() {
      let diff = main_lines[i].start_time as i64 - trans[j].start_time as i64;
      if diff.abs() <= ALIGN_TOLERANCE_MS {
        let trans_text = trans[j].text();
        if is_meaningful_subline(&trans_text) {
          main_lines[i].translated_lyric = Some(trans_text);
        }
        i += 1;
        j += 1;
      } else if diff < 0 {
        i += 1;
      } else {
        j += 1;
      }
    }
  }

  // 2. 合并罗马音歌词
  if let Some(mut romaji) = roman_lines {
    romaji.sort_by_key(|l| l.start_time);
    let mut i = 0;
    let mut j = 0;
    while i < main_lines.len() && j < romaji.len() {
      let diff = main_lines[i].start_time as i64 - romaji[j].start_time as i64;
      if diff.abs() <= ALIGN_TOLERANCE_MS {
        let roman_text = romaji[j].text();
        if is_meaningful_subline(&roman_text) {
          main_lines[i].roman_lyric = Some(roman_text);
        }
        i += 1;
        j += 1;
      } else if diff < 0 {
        i += 1;
      } else {
        j += 1;
      }
    }
  }

  main_lines
}
