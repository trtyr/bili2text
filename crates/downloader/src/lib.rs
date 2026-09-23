//! 平台下载能力：视频 → 16k mono WAV 音频（供 ASR 转写）。
//!
//! 实现：yt-dlp 拉取最佳音轨（可携带平台登录态 cookie，解锁高音质），
//! ffmpeg 转码为单声道 16kHz WAV。两者均为外部进程，路径可用环境变量覆盖：
//! - `YTDLP_BIN`（默认 `yt-dlp`）
//! - `FFMPEG_BIN`（默认 `ffmpeg`）

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("外部依赖缺失：{0}（请安装后重试，或用环境变量指定路径）")]
    MissingBinary(&'static str),
    #[error("yt-dlp 下载失败：{0}")]
    YtDlp(String),
    #[error("ffmpeg 转码失败：{0}")]
    Ffmpeg(String),
}

pub type Result<T> = std::result::Result<T, DownloadError>;

/// 音频下载器。`work_dir` 存放中间文件（m4a）与产出 WAV。
pub struct AudioDownloader {
    work_dir: PathBuf,
    ytdlp_bin: String,
    ffmpeg_bin: String,
}

impl AudioDownloader {
    pub fn new(work_dir: impl Into<PathBuf>) -> Self {
        Self {
            work_dir: work_dir.into(),
            ytdlp_bin: std::env::var("YTDLP_BIN").unwrap_or_else(|_| "yt-dlp".into()),
            ffmpeg_bin: std::env::var("FFMPEG_BIN").unwrap_or_else(|_| "ffmpeg".into()),
        }
    }

    /// 下载视频的最佳音轨并转码为 16k mono WAV，返回 WAV 路径。
    ///
    /// - `page_url`：视频页地址（https://www.bilibili.com/video/{bvid}）
    /// - `cookie_file`：可选 Netscape cookie 文件（平台登录态，解锁高音质）
    /// - `file_stem`：输出文件名主干
    pub async fn fetch_wav(
        &self,
        page_url: &str,
        cookie_file: Option<&Path>,
        file_stem: &str,
    ) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.work_dir).ok();

        let m4a = self.work_dir.join(format!("{file_stem}.m4a"));
        let wav = self.work_dir.join(format!("{file_stem}.wav"));

        // 1) yt-dlp 拉最佳音轨
        let mut cmd = Command::new(&self.ytdlp_bin);
        cmd.arg("-f")
            .arg("bestaudio")
            .arg("--no-playlist")
            .arg("--no-progress")
            .arg("-o")
            .arg(&m4a)
            .arg(page_url);
        if let Some(cookie) = cookie_file {
            cmd.arg("--cookies").arg(cookie);
        }
        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    DownloadError::MissingBinary("yt-dlp")
                } else {
                    DownloadError::YtDlp(e.to_string())
                }
            })?;
        if !output.status.success() {
            return Err(DownloadError::YtDlp(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }

        // 2) ffmpeg 转 16k mono wav
        let output = Command::new(&self.ffmpeg_bin)
            .args(["-y", "-i"])
            .arg(&m4a)
            .args(["-vn", "-ac", "1", "-ar", "16000"])
            .arg(&wav)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    DownloadError::MissingBinary("ffmpeg")
                } else {
                    DownloadError::Ffmpeg(e.to_string())
                }
            })?;
        if !output.status.success() {
            return Err(DownloadError::Ffmpeg(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }

        // 3) 清理中间文件
        std::fs::remove_file(&m4a).ok();
        Ok(wav)
    }
}
