pub mod aligner;
pub mod model;
pub mod parser_lrc;
pub mod parser_ttml;
pub mod parser_yrc;
pub mod timeline;

pub use aligner::align_lyrics;
pub use model::{InterludeState, LyricLine, LyricSpan, LyricWord};
pub use parser_lrc::parse_lrc;
pub use parser_ttml::parse_ttml;
pub use parser_yrc::parse_yrc;
pub use timeline::LyricTimeline;

pub fn parse_lyrics_bundle(
  lrc: Option<&str>,
  yrc: Option<&str>,
  translation: Option<&str>,
  romaji: Option<&str>,
) -> Vec<LyricLine> {
  // 1. 确定主歌词格式并解析
  let main_lines = if let Some(yrc_text) = yrc.filter(|s| !s.trim().is_empty()) {
    let lines = parse_yrc(yrc_text);
    if lines.is_empty() {
      if let Some(lrc_text) = lrc {
        parse_lrc(lrc_text)
      } else {
        Vec::new()
      }
    } else {
      lines
    }
  } else if let Some(lrc_text) = lrc.filter(|s| !s.trim().is_empty()) {
    parse_lrc(lrc_text)
  } else {
    Vec::new()
  };

  if main_lines.is_empty() {
    return Vec::new();
  }

  // 2. 解析辅助的翻译歌词
  let trans_lines = translation.filter(|s| !s.trim().is_empty()).map(parse_lrc);

  // 3. 解析辅助的罗马音歌词
  let roman_lines = romaji.filter(|s| !s.trim().is_empty()).map(parse_lrc);

  // 4. 对齐合并
  align_lyrics(main_lines, trans_lines, roman_lines)
}
