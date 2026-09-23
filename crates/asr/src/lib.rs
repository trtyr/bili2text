//! 平台 ASR 能力：SenseVoice 本地转写（sherpa-onnx，经 sherpa-rs，纯 Rust 进程内推理）。
//!
//! 输入：16k mono WAV（由平台下载能力 + ffmpeg 产出）。
//! 输出：全文 + 按 token 时间戳切句的分段（含 SRT 文本）。
//!
//! 模型：sherpa-onnx 官方 SenseVoice Small int8（中/英/日/韩/粤），
//! 放置于 `data/models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/`。

use std::path::Path;

use hound::{SampleFormat, WavReader};
use sherpa_rs::sense_voice::{SenseVoiceConfig, SenseVoiceRecognizer};

const DEFAULT_MODEL_DIR: &str =
    "data/models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17";

#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    #[error("模型未找到：{path}（运行 scripts/download-model.sh 下载）")]
    ModelMissing { path: String },
    #[error("音频读取失败：{0}")]
    Audio(String),
    #[error("推理失败：{0}")]
    Inference(String),
}

pub type Result<T> = std::result::Result<T, AsrError>;

/// 转写结果的一个句段。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Segment {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

/// 完整转写结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Transcription {
    pub lang: String,
    pub text: String,
    pub segments: Vec<Segment>,
    pub srt: String,
}

/// SenseVoice 推理引擎。
pub struct AsrEngine {
    recognizer: SenseVoiceRecognizer,
}

impl AsrEngine {
    /// 从模型目录加载（int8 模型 + tokens）。加载约 1-2 秒，建议进程内复用。
    pub fn open(model_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = model_dir.as_ref();
        let model = dir.join("model.int8.onnx");
        let tokens = dir.join("tokens.txt");
        for f in [&model, &tokens] {
            if !f.exists() {
                return Err(AsrError::ModelMissing {
                    path: dir.display().to_string(),
                });
            }
        }

        let recognizer = SenseVoiceRecognizer::new(SenseVoiceConfig {
            model: model.display().to_string(),
            tokens: tokens.display().to_string(),
            language: "auto".to_string(),
            use_itn: true,
            num_threads: Some(0), // 0 = 自动
            debug: false,
            provider: Some("cpu".to_string()),
        })
        .map_err(|e| AsrError::Inference(e.to_string()))?;

        Ok(Self { recognizer })
    }

    /// 按默认模型目录加载。
    pub fn open_default() -> Result<Self> {
        Self::open(DEFAULT_MODEL_DIR)
    }

    /// 转写 16k mono WAV 文件。
    pub fn transcribe_wav(&mut self, wav_path: impl AsRef<Path>) -> Result<Transcription> {
        let (samples, sample_rate) = read_wav_16k_mono(wav_path.as_ref())?;
        self.transcribe_samples(samples, sample_rate)
    }

    /// 转写 f32 PCM 采样。
    pub fn transcribe_samples(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<Transcription> {
        let result = self.recognizer.transcribe(sample_rate, &samples);

        let segments = segment_tokens(&result.tokens, &result.timestamps);
        let text = normalize_text(&result.text);
        let srt = to_srt(&segments);

        Ok(Transcription {
            lang: result.lang,
            text,
            segments,
            srt,
        })
    }
}

/// 读取 WAV（要求 16k mono，下载能力负责转码），返回 f32 采样。
fn read_wav_16k_mono(path: &Path) -> Result<(Vec<f32>, u32)> {
    let reader = WavReader::open(path).map_err(|e| AsrError::Audio(e.to_string()))?;
    let spec = reader.spec();
    if spec.channels != 1 {
        return Err(AsrError::Audio(format!(
            "期望单声道，实际 {} 声道（应由下载能力先转码）",
            spec.channels
        )));
    }
    let sample_rate = spec.sample_rate;
    let samples: Vec<f32> = match spec.sample_format {
        SampleFormat::Int => reader
            .into_samples::<i16>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AsrError::Audio(e.to_string()))?
            .into_iter()
            .map(|s| s as f32 / 32768.0)
            .collect(),
        SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AsrError::Audio(e.to_string()))?,
    };
    Ok((samples, sample_rate))
}

/// token + 时间戳 → 句段：按终止标点或 ≥1.5s 停顿切句。
fn segment_tokens(tokens: &[String], timestamps: &[f32]) -> Vec<Segment> {
    const PAUSE_CUT: f32 = 1.5;
    const ENDINGS: &[char] = &['。', '！', '？', '；', '.', '!', '?', ';', '，', ','];

    let mut segments = Vec::new();
    let mut cur = String::new();
    let mut cur_start: Option<f32> = None;
    let mut last_ts: Option<f32> = None;

    for (tok, ts) in tokens.iter().zip(timestamps.iter()) {
        if let Some(prev) = last_ts {
            if ts - prev >= PAUSE_CUT && !cur.trim().is_empty() {
                segments.push(Segment {
                    start: cur_start.unwrap_or(prev),
                    end: prev + 0.5,
                    text: std::mem::take(&mut cur),
                });
                cur_start = None;
            }
        }
        if cur_start.is_none() {
            cur_start = Some(*ts);
        }
        cur.push_str(tok);
        let is_ending = tok.chars().last().is_some_and(|c| ENDINGS.contains(&c));
        last_ts = Some(*ts);
        if is_ending && !cur.trim().is_empty() {
            segments.push(Segment {
                start: cur_start.unwrap_or(*ts),
                end: *ts + 0.4,
                text: std::mem::take(&mut cur),
            });
            cur_start = None;
        }
    }
    if !cur.trim().is_empty() {
        segments.push(Segment {
            start: cur_start.unwrap_or(0.0),
            end: last_ts.unwrap_or(0.0) + 0.4,
            text: cur,
        });
    }
    segments
}

/// 模型输出文本去除语言/情感/事件标记（如 <|zh|><|NEUTRAL|>）。
fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            for skip in chars.by_ref() {
                if skip == '>' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

fn to_srt(segments: &[Segment]) -> String {
    fn stamp(secs: f32) -> String {
        let total_ms = (secs.max(0.0) * 1000.0).round() as u64;
        let h = total_ms / 3_600_000;
        let m = (total_ms % 3_600_000) / 60_000;
        let s = (total_ms % 60_000) / 1000;
        let ms = total_ms % 1000;
        format!("{h:02}:{m:02}:{s:02},{ms:03}")
    }
    segments
        .iter()
        .enumerate()
        .map(|(i, seg)| {
            format!(
                "{}\n{} --> {}\n{}\n",
                i + 1,
                stamp(seg.start),
                stamp(seg.end),
                seg.text
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_tags() {
        assert_eq!(
            normalize_text("<|zh|><|NEUTRAL|><|Speech|>你好世界<|END|>"),
            "你好世界"
        );
        assert_eq!(normalize_text("无标记"), "无标记");
    }

    #[test]
    fn segment_by_pause_and_punct() {
        let tokens: Vec<String> = ["你", "好", "。", "世", "界", "！"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let stamps = vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0];
        let segs = segment_tokens(&tokens, &stamps);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "你好。");
        assert_eq!(segs[1].text, "世界！");

        // 停顿切句
        let stamps = vec![0.0, 0.2, 0.4, 3.0, 3.2, 3.4];
        let segs = segment_tokens(&tokens, &stamps);
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn model_present() {
        // 模型已下载（新环境需先跑 scripts/download-model.sh）
        let dir = Path::new("data/models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17");
        if dir.exists() {
            assert!(AsrEngine::open(dir).is_ok());
        }
    }
}
