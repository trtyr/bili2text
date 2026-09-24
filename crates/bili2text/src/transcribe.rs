//! 本地转写：下载音频 → ffmpeg 转码 → SenseVoice 推理。

use std::path::Path;
use std::sync::Mutex;

use bili_client::BiliClient;
use downloader::AudioDownloader;

use crate::error::AppError;
use crate::output::Doc;
use crate::tasks::TaskStore;
use crate::log_step;

/// 标准库 Result 别名（本文件局部，避免与 AppError 版 Result 混淆）。
type StdResult<T, E> = std::result::Result<T, E>;

type Result<T> = std::result::Result<T, AppError>;

/// SenseVoice 模型目录名（位于数据目录 models/ 下）。
pub const MODEL_SUBDIR: &str = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17";

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

/// ASR 流水线参数。
pub struct TranscribeReq<'a> {
    pub bvid: &'a str,
    pub title: &'a str,
    pub duration_secs: u64,
    /// 数据目录（放 cookie 导出文件与临时音频）。
    pub data_dir: &'a Path,
}

pub async fn run(
    client: &BiliClient,
    downloader: &AudioDownloader,
    asr: &std::sync::Arc<AsrSlot>,
    store: &TaskStore,
    task_id: &str,
    req: &TranscribeReq<'_>,
) -> Result<Doc> {
    // 阶段 1：下载音频（携带登录态 cookie 解锁高音质）
    log_step!("[1/2] 下载音频（yt-dlp + ffmpeg 转码 16k mono）…");
    store
        .update_progress(task_id, "downloading", 10)
        .map_err(|e| AppError::Storage(e.to_string()))?;
    let cookie_file = client
        .credential()
        .to_netscape_file(&req.data_dir.join("bili-cookies.txt"))
        .map_err(|e| AppError::Storage(format!("导出 cookie 失败：{e}")))?;
    let page_url = format!("https://www.bilibili.com/video/{}", req.bvid);
    let wav = downloader
        .fetch_wav(
            &page_url,
            cookie_file.as_deref().map(Path::new),
            req.bvid,
        )
        .await
        .map_err(AppError::from_download)?;

    // 阶段 2：本地推理（CPU 密集，放阻塞线程池）
    log_step!("[2/2] SenseVoice 本地转写中（CPU，约需几分钟）…");
    store
        .update_progress(task_id, "transcribing", 50)
        .map_err(|e| AppError::Storage(e.to_string()))?;
    let wav_path = wav.clone();
    let asr_slot = std::sync::Arc::clone(asr);
    let transcription = tokio::task::spawn_blocking(move || {
        asr_slot.with_engine(|engine| engine.transcribe_wav(&wav_path))
    })
    .await
    .map_err(|e| AppError::Other(format!("推理任务中断：{e}")))?
    .map_err(|e| AppError::Asr(e.to_string()))?;

    // 清理音频文件（结果已保存）
    tokio::fs::remove_file(&wav).await.ok();

    // lang 形如 "<|zh|>"，剥掉标记
    let lang: String = transcription
        .lang
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_string();

    log_step!(
        "转写完成：{} 段，{} 字符",
        transcription.segments.len(),
        transcription.text.chars().count()
    );

    Ok(Doc {
        title: req.title.to_string(),
        bvid: req.bvid.to_string(),
        duration_secs: req.duration_secs,
        lang: lang.clone(),
        source: format!("本地转写（SenseVoice, {lang}）"),
        lines_count: transcription.segments.len(),
        text: transcription.text,
        srt: transcription.srt,
    })
}
