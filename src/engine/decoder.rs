use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::{AudioDecoder as SymphoniaAudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

use super::source::AudioSourceStream;
use crate::common::error::{AppError, AppResult};

/// 音频解码器
pub struct AudioDecoder {
  format: Box<dyn symphonia::core::formats::FormatReader>,
  decoder: Box<dyn SymphoniaAudioDecoder>,
  track_id: u32,
  sample_rate: u32,
  channels: u16,
  duration_ms: u64,
}

impl AudioDecoder {
  /// 从音频源创建解码器实例
  pub fn new(source: AudioSourceStream) -> AppResult<Self> {
    let mss = MediaSourceStream::new(Box::new(source), MediaSourceStreamOptions::default());
    let hint = Hint::new();

    let format = symphonia::default::get_probe()
      .probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
      )
      .map_err(|e| AppError::AudioEngine(format!("音频容器探测失败: {e}")))?;

    let track = format
      .tracks()
      .iter()
      .find(|track| match &track.codec_params {
        Some(CodecParameters::Audio(params)) => {
          params.codec != symphonia::core::codecs::audio::CODEC_ID_NULL_AUDIO
        }
        _ => false,
      })
      .ok_or_else(|| AppError::AudioEngine("未在音源中找到有效的音频轨道".to_string()))?;

    let track_id = track.id;
    let audio_params = match &track.codec_params {
      Some(CodecParameters::Audio(params)) => params,
      _ => unreachable!(),
    };

    let sample_rate = audio_params.sample_rate.unwrap_or(44_100);
    let channels = audio_params
      .channels
      .as_ref()
      .map(|c| c.count() as u16)
      .unwrap_or(2);

    // 从 track.time_base 和 track.duration 计算总时长 (毫秒)
    let duration_ms = if let (Some(tb), Some(dur)) = (track.time_base, track.duration) {
      tb.calc_duration(dur)
        .map(|t| t.as_millis().max(0) as u64)
        .unwrap_or(0)
    } else {
      0
    };

    let decoder = symphonia::default::get_codecs()
      .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
      .map_err(|e| AppError::AudioEngine(format!("不支持的音频编码或初始化失败: {e}")))?;

    Ok(Self {
      format,
      decoder,
      track_id,
      sample_rate,
      channels,
      duration_ms,
    })
  }

  pub fn sample_rate(&self) -> u32 {
    self.sample_rate
  }

  pub fn channels(&self) -> u16 {
    // 解码器在 next_chunk 中统一将单声道复制扩展并多声道下混为标准双声道立体声 (2 channels)
    2
  }

  pub fn duration_ms(&self) -> u64 {
    self.duration_ms
  }

  /// 跳转到指定毫秒位置
  pub fn seek_ms(&mut self, ms: u64) -> AppResult<()> {
    let time =
      symphonia::core::units::Time::try_from_secs_f64(ms as f64 / 1000.0).unwrap_or_default();
    let seek_to = symphonia::core::formats::SeekTo::Time {
      time,
      track_id: Some(self.track_id),
    };
    let _ = self
      .format
      .seek(symphonia::core::formats::SeekMode::Coarse, seek_to)
      .map_err(|e| AppError::AudioEngine(format!("Seek失败: {e}")))?;
    self.decoder.reset();
    Ok(())
  }

  /// 流式解码下一个数据包，输出为标准立体声 (2 channels) 交错 f32 采样点切片
  pub fn next_chunk(&mut self) -> Result<Option<Vec<f32>>, SymphoniaError> {
    loop {
      let packet = match self.format.next_packet() {
        Ok(Some(packet)) => packet,
        Ok(None) => return Ok(None),
        Err(SymphoniaError::IoError(ref err))
          if err.kind() == std::io::ErrorKind::UnexpectedEof =>
        {
          return Ok(None);
        }
        Err(SymphoniaError::ResetRequired) => {
          self.decoder.reset();
          continue;
        }
        Err(err) => return Err(err),
      };

      if packet.track_id != self.track_id {
        continue;
      }

      match self.decoder.decode(&packet) {
        Ok(audio_buf_ref) => {
          let mut chunk = Vec::new();
          audio_buf_ref.copy_to_vec_interleaved(&mut chunk);

          let mut out = Vec::with_capacity(chunk.len() * 2);
          if self.channels == 1 {
            for &s in &chunk {
              out.push(s);
              out.push(s);
            }
          } else if self.channels == 2 {
            out = chunk;
          } else {
            let n_channels = self.channels as usize;
            for frame in chunk.chunks(n_channels) {
              if frame.len() >= 2 {
                out.push(frame[0]);
                out.push(frame[1]);
              }
            }
          }
          return Ok(Some(out));
        }
        Err(SymphoniaError::DecodeError(err)) => {
          tracing::debug!("跳过损坏的音频帧: {err}");
          continue;
        }
        Err(err) => return Err(err),
      }
    }
  }
}
