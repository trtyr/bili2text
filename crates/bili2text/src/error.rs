//! 统一错误模型：按类别分退出码，底层工具错误原文透传。
//!
//! 退出码约定：
//! - `1`  其他未分类错误
//! - `2`  用法 / 参数问题
//! - `10` 无法从输入解析 BV 号
//! - `20` B 站接口返回业务错误（code/message 原样保留）
//! - `21` 网络请求失败（reqwest 错误原样保留）
//! - `22` B 站响应解析失败
//! - `30` 未登录或登录态失效
//! - `31` 视频没有可用字幕
//! - `40` 外部依赖缺失（yt-dlp / ffmpeg 未安装）
//! - `41` yt-dlp 下载失败（stderr 原样透传）
//! - `42` ffmpeg 转码失败（stderr 原样透传）
//! - `50` 本地转写引擎错误（原文透传）
//! - `60` 本地存储 / IO 错误

use bili_client::BiliError;
#[cfg(feature = "transcribe")]
use downloader::DownloadError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 用法 / 参数问题（2）。
    #[error("{0}")]
    Usage(String),

    /// 无法从输入解析 BV 号（10），携带原始输入。
    #[error("无法从输入解析出 BV 号：{0}")]
    BadInput(String),

    /// B 站接口返回业务错误（20），code 与 message 原样透传。
    #[error("B 站接口返回错误 code={code}: {message}")]
    BiliApi { code: i64, message: String },

    /// 网络请求失败（21），reqwest 错误原样透传。
    #[error("网络请求失败：{0}")]
    BiliNetwork(String),

    /// B 站响应不符合预期 / JSON 解析失败（22）。
    #[error("B 站响应解析失败：{0}")]
    BiliData(String),

    /// 未登录或登录态失效（30）。
    #[error("未登录或登录态失效。AI 字幕与高音质下载需要登录：bili2text login")]
    NotLoggedIn,

    /// 视频没有任何可用字幕轨（31）。
    #[error(
        "该视频没有可用字幕（AI 字幕需要登录，且并非所有视频都生成过）。\n提示：加 --transcribe 可用本地 SenseVoice 模型转写"
    )]
    NoSubtitle { bvid: String, title: String },

    /// 外部依赖缺失（40）：模型、yt-dlp、ffmpeg 等。
    #[cfg(feature = "transcribe")]
    #[error("外部依赖缺失：{0}（可运行 bili2text doctor --fix 检测并自动修复）")]
    ExternalMissing(&'static str),

    /// yt-dlp 下载失败（41），进程 stderr 原样透传。
    #[cfg(feature = "transcribe")]
    #[error("yt-dlp 下载失败：\n{0}")]
    YtDlpFailed(String),

    /// ffmpeg 转码失败（42），进程 stderr 原样透传。
    #[cfg(feature = "transcribe")]
    #[error("ffmpeg 转码失败：\n{0}")]
    FfmpegFailed(String),

    /// 本地转写引擎错误（50），原文透传。
    #[cfg(feature = "transcribe")]
    #[error("本地转写失败：{0}")]
    Asr(String),

    /// 本地存储 / IO（60）。
    #[error("本地存储错误：{0}")]
    Storage(String),

    /// 其他未分类错误（1）。
    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// 对应的进程退出码。
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::BadInput(_) => 10,
            Self::BiliApi { .. } => 20,
            Self::BiliNetwork(_) => 21,
            Self::BiliData(_) => 22,
            Self::NotLoggedIn => 30,
            Self::NoSubtitle { .. } => 31,
            #[cfg(feature = "transcribe")]
            Self::ExternalMissing(_) => 40,
            #[cfg(feature = "transcribe")]
            Self::YtDlpFailed(_) => 41,
            #[cfg(feature = "transcribe")]
            Self::FfmpegFailed(_) => 42,
            #[cfg(feature = "transcribe")]
            Self::Asr(_) => 50,
            Self::Storage(_) => 60,
            Self::Other(_) => 1,
        }
    }

    /// B 站客户端错误逐变体映射：每个变体原文保留，不合并归类。
    pub fn from_bili(e: BiliError) -> Self {
        match e {
            BiliError::BadInput(input) => Self::BadInput(input),
            BiliError::NotLoggedIn => Self::NotLoggedIn,
            BiliError::Api { code, message } => Self::BiliApi { code, message },
            BiliError::Network(e) => Self::BiliNetwork(e.to_string()),
            BiliError::Io(e) => Self::Storage(e.to_string()),
            BiliError::Json(e) => Self::BiliData(e.to_string()),
        }
    }

    /// 下载器错误逐变体映射：yt-dlp / ffmpeg 的原始 stderr 原样透传。
    #[cfg(feature = "transcribe")]
    pub fn from_download(e: DownloadError) -> Self {
        match e {
            DownloadError::MissingBinary(name) => Self::ExternalMissing(name),
            DownloadError::YtDlp(stderr) => Self::YtDlpFailed(stderr),
            DownloadError::Ffmpeg(stderr) => Self::FfmpegFailed(stderr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_distinct_per_category() {
        #[cfg_attr(not(feature = "transcribe"), allow(unused_mut))]
        let mut cases: Vec<(AppError, i32)> = vec![
            (AppError::Usage("x".into()), 2),
            (AppError::BadInput("BVxx".into()), 10),
            (
                AppError::BiliApi { code: -404, message: "啥都木有".into() },
                20,
            ),
            (AppError::BiliNetwork("timeout".into()), 21),
            (AppError::BiliData("bad json".into()), 22),
            (AppError::NotLoggedIn, 30),
            (
                AppError::NoSubtitle { bvid: "BV1".into(), title: "t".into() },
                31,
            ),
            (AppError::Storage("disk".into()), 60),
            (AppError::Other("misc".into()), 1),
        ];
        #[cfg(feature = "transcribe")]
        cases.extend([
            (AppError::ExternalMissing("yt-dlp"), 40),
            (AppError::YtDlpFailed("boom".into()), 41),
            (AppError::FfmpegFailed("boom".into()), 42),
            (AppError::Asr("no model".into()), 50),
        ]);
        for (e, code) in cases {
            assert_eq!(e.exit_code(), code, "{e:?}");
        }
    }

    #[test]
    fn from_bili_keeps_original_text() {
        let e = AppError::from_bili(BiliError::BadInput("原输入".into()));
        assert!(matches!(e, AppError::BadInput(ref s) if s == "原输入"));

        let e = AppError::from_bili(BiliError::Api { code: -352, message: "风控".into() });
        assert!(matches!(
            e,
            AppError::BiliApi { ref code, ref message } if *code == -352 && message == "风控"
        ));
    }

    #[test]
    #[cfg(feature = "transcribe")]
    fn from_download_keeps_stderr() {
        let e = AppError::from_download(DownloadError::YtDlp("原始 stderr 内容".into()));
        assert!(matches!(e, AppError::YtDlpFailed(ref s) if s == "原始 stderr 内容"));

        let e = AppError::from_download(DownloadError::MissingBinary("ffmpeg"));
        assert!(matches!(e, AppError::ExternalMissing("ffmpeg")));
    }
}
