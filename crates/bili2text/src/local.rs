//! 本地转写引擎：SenseVoice int8（sherpa-rs 进程内推理）。
//!
//! 仅 `transcribe` feature 构建可用；模型目录位于数据目录 `models/` 下。

use std::path::Path;
use std::sync::Mutex;

use crate::error::AppError;
use crate::transcriber::{Segment, Transcript};

type StdResult<T, E> = std::result::Result<T, E>;

/// SenseVoice 模型目录名（位于数据目录 models/ 下）。
pub const MODEL_SUBDIR: &str = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17";

/// 模型主文件是否存在（缺失时给出可行动错误，而非推理时底层报错）。
pub fn model_present(data_dir: &Path) -> bool {
    data_dir
        .join("models")
        .join(MODEL_SUBDIR)
        .join("model.int8.onnx")
        .exists()
}

/// ASR 引擎共享槽：懒加载 + 串行推理（CPU 密集，无并行收益）。
#[derive(Default)]
pub struct AsrSlot {
    model_dir: Option<std::path::PathBuf>,
    engine: Mutex<Option<asr::AsrEngine>>,
}

impl AsrSlot {
    /// 指定模型目录（显式路径，不依赖 cwd）。
    pub fn with_model_dir(model_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            model_dir: Some(model_dir.into()),
            engine: Mutex::new(None),
        }
    }

    /// 借出引擎（首次调用时加载模型，约 1-2 秒）。
    fn with_engine<T>(
        &self,
        f: impl FnOnce(&mut asr::AsrEngine) -> StdResult<T, asr::AsrError>,
    ) -> StdResult<T, asr::AsrError> {
        let mut guard = self.engine.lock().unwrap();
        if guard.is_none() {
            let engine = match &self.model_dir {
                Some(dir) => asr::AsrEngine::open(dir)?,
                None => asr::AsrEngine::open_default()?,
            };
            *guard = Some(engine);
        }
        f(guard.as_mut().unwrap())
    }
}

/// 本地 SenseVoice 转写引擎。
pub struct LocalTranscriber {
    slot: std::sync::Arc<AsrSlot>,
}

impl LocalTranscriber {
    /// 指定模型目录构造。
    pub fn new(model_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            slot: std::sync::Arc::new(AsrSlot::with_model_dir(model_dir)),
        }
    }

    pub fn label(&self) -> &'static str {
        "SenseVoice"
    }

    /// 转写一个 16k mono WAV（推理放阻塞线程池，模型懒加载串行复用）。
    pub async fn transcribe(&self, wav: &Path) -> Result<Transcript, AppError> {
        let slot = std::sync::Arc::clone(&self.slot);
        let wav = wav.to_path_buf();
        let result = tokio::task::spawn_blocking(move || {
            slot.with_engine(|engine| engine.transcribe_wav(&wav))
        })
        .await
        .map_err(|e| AppError::Other(format!("推理任务中断：{e}")))?
        .map_err(|e| AppError::Asr(e.to_string()))?;

        // lang 形如 "<|zh|>"，剥掉标记
        let lang: String = result
            .lang
            .trim_matches(|c: char| !c.is_ascii_alphanumeric())
            .to_string();

        Ok(Transcript {
            lang: lang.clone(),
            text: result.text,
            segments: result
                .segments
                .into_iter()
                .map(|s| Segment { start: s.start, end: s.end, text: s.text })
                .collect(),
            srt: result.srt,
        })
    }
}
