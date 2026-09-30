//! 转写引擎抽象：本地 SenseVoice 与远程转写服务统一接口。
//!
//! 引擎选择在编排层完成（main.rs `select_engine`），本模块提供
//! 结果类型与枚举分发——实现固定两种，用 enum 分发规避 dyn + async trait。

use std::path::Path;

use crate::error::AppError;

/// 一条转写句段（秒级时间轴）。
#[derive(Debug, Clone)]
pub struct Segment {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

/// 转写结果（引擎无关）：全文 + 句段 + SRT。
#[derive(Debug, Clone)]
pub struct Transcript {
    pub lang: String,
    pub text: String,
    pub segments: Vec<Segment>,
    pub srt: String,
}

/// 本地转写引擎（SenseVoice 进程内推理，仅 `transcribe` feature 构建）。
#[cfg(feature = "transcribe")]
pub use crate::local::LocalTranscriber;

/// 远程转写引擎（HTTP 服务，任意构建可用）。
pub use crate::remote::RemoteTranscriber;

/// 转写引擎枚举分发。
pub enum Engine {
    /// 本地 SenseVoice（仅转写构建可构造）。
    #[cfg(feature = "transcribe")]
    Local(LocalTranscriber),
    /// 远程转写服务。
    Remote(RemoteTranscriber),
}

impl Engine {
    /// 场所标签（文档「文本来源」前缀）：本地 / 远程。
    pub fn scope(&self) -> &'static str {
        match self {
            #[cfg(feature = "transcribe")]
            Engine::Local(_) => "本地",
            Engine::Remote(_) => "远程",
        }
    }

    /// 引擎名（文档「文本来源」用），如 "SenseVoice" / "Qwen3-ASR"。
    pub fn label(&self) -> String {
        match self {
            #[cfg(feature = "transcribe")]
            Engine::Local(e) => e.label().to_string(),
            Engine::Remote(e) => e.label().to_string(),
        }
    }

    /// 转写一个 16k mono WAV 文件。
    pub async fn transcribe(&self, wav: &Path) -> Result<Transcript, AppError> {
        match self {
            #[cfg(feature = "transcribe")]
            Engine::Local(e) => e.transcribe(wav).await,
            Engine::Remote(e) => e.transcribe(wav).await,
        }
    }
}
