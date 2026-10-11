use slint::{ComponentHandle, VecModel};
use std::rc::Rc;

use crate::bridge::MainWindow;
use crate::bridge::{LyricLineUi, LyricState, LyricWordUi};
use crate::lyric::LyricLine;

/// 歌词行模型与逐字点亮时间轴同步器
pub struct SyncLyric;

impl SyncLyric {
  /// 载入解析后的歌词并同步到 Slint UI 模型
  pub fn sync_lyrics(ui: &MainWindow, lines: &[LyricLine]) {
    let global = ui.global::<LyricState>();
    if lines.is_empty() {
      global.set_has_lyrics(false);
      global.set_lines(Rc::new(VecModel::default()).into());
      global.set_active_index(-1);
      global.set_word_progress(0.0);
      global.set_is_interlude(false);
      return;
    }

    let ui_lines: Vec<LyricLineUi> = lines
      .iter()
      .map(|l| {
        let words: Vec<LyricWordUi> = l
          .words
          .iter()
          .map(|w| LyricWordUi {
            word: w.word.clone().into(),
            start_time_ms: w.start_time as i32,
            end_time_ms: w.end_time as i32,
          })
          .collect();
        LyricLineUi {
          main_text: l.text().into(),
          trans_text: l.translated_lyric.clone().unwrap_or_default().into(),
          roman_text: l.roman_lyric.clone().unwrap_or_default().into(),
          start_time_ms: l.start_time as i32,
          end_time_ms: l.end_time as i32,
          is_active: false,
          is_bg: l.is_bg,
          words: Rc::new(VecModel::from(words)).into(),
        }
      })
      .collect();

    global.set_has_lyrics(true);
    let model = Rc::new(VecModel::from(ui_lines));
    global.set_lines(model.into());
    global.set_active_index(0);
    global.set_word_progress(0.0);
  }

  /// 同步歌词播放实时进度
  pub fn sync_progress(
    ui: &MainWindow,
    active_index: i32,
    word_progress: f32,
    is_interlude: bool,
    interlude_dots: i32,
  ) {
    let global = ui.global::<LyricState>();
    global.set_active_index(active_index);
    global.set_word_progress(word_progress);
    global.set_is_interlude(is_interlude);
    global.set_interlude_dots(interlude_dots);
  }
}
