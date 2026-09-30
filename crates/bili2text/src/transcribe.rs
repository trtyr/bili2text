//! 转写管线：下载音频（yt-dlp + ffmpeg）→ 交给引擎（本地/远程）→ 组装 Doc。
//!
//! 本模块与引擎无关：本地 SenseVoice 与远程服务都走同一条下载与组装路径。

use std::path::Path;

use bili_client::BiliClient;
use downloader::AudioDownloader;

use crate::error::AppError;
use crate::output::Doc;
use crate::tasks::TaskStore;
use crate::transcriber::Engine;

/// 转写管线参数（bvid/title/时长由编排层确定）。
pub struct PipelineReq<'a> {
    pub bvid: &'a str,
    pub title: &'a str,
    /// 多 P 时的分 P 号（拼进下载 URL，yt-dlp 按网页语义取对应 P）。
    pub page: Option<u64>,
    pub duration_secs: u64,
    /// 数据目录（放 cookie 导出文件与临时音频）。
    pub data_dir: &'a Path,
}

/// 执行转写管线：下载 → 引擎 → 清理 → Doc。
pub async fn run(
    client: &BiliClient,
    engine: &Engine,
    store: &TaskStore,
    task_id: &str,
    req: &PipelineReq<'_>,
) -> Result<Doc, AppError> {
    // 阶段 1：下载音频（携带登录态 cookie 解锁高音质）
    crate::log_step!("[1/2] 下载音频（yt-dlp + ffmpeg 转码 16k mono）…");
    store
        .update_progress(task_id, "downloading", 10)
        .map_err(|e| AppError::Storage(e.to_string()))?;
    let cookie_file = client
        .credential()
        .to_netscape_file(&req.data_dir.join("bili-cookies.txt"))
        .map_err(|e| AppError::Storage(format!("导出 cookie 失败：{e}")))?;
    let page_url = match req.page {
        Some(n) => format!("https://www.bilibili.com/video/{}?p={}", req.bvid, n),
        None => format!("https://www.bilibili.com/video/{}", req.bvid),
    };
    let downloader = AudioDownloader::new(req.data_dir.join("audio"));
    let wav = downloader
        .fetch_wav(
            &page_url,
            cookie_file.as_deref().map(Path::new),
            req.bvid,
        )
        .await
        .map_err(AppError::from_download)?;

    // 阶段 2：引擎转写（本地进程内 或 远程服务）
    crate::log_step!("[2/2] 转写中（引擎：{}）…", engine.label());
    store
        .update_progress(task_id, "transcribing", 50)
        .map_err(|e| AppError::Storage(e.to_string()))?;
    let transcript = engine.transcribe(&wav).await?;

    // 清理音频文件（结果已保存）
    tokio::fs::remove_file(&wav).await.ok();

    Ok(Doc {
        title: req.title.to_string(),
        bvid: req.bvid.to_string(),
        duration_secs: req.duration_secs,
        lang: transcript.lang.clone(),
        source: format!("{}转写（{}, {}）", engine.scope(), engine.label(), transcript.lang),
        lines_count: transcript.segments.len(),
        text: transcript.text,
        srt: transcript.srt,
    })
}
