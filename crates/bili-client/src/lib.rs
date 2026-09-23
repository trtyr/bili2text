//! B 站开放接口客户端。
//!
//! 模块划分：
//! - [`wbi`]：WBI 参数签名（`x/player/wbi/v2` 等接口必需）
//! - [`credential`]：登录态（cookie 集合 + 文件持久化）
//! - [`qrcode`]：扫码登录（generate / poll）
//! - [`video`]：视频信息与字幕拉取
//!
//! 接口依据 bilibili-API-collect 文档；wbi 算法含官方测试向量单测。

pub mod credential;
pub mod qrcode;
pub mod video;
pub mod wbi;

use std::path::PathBuf;
use std::sync::RwLock;

pub use credential::Credential;

const API_BASE: &str = "https://api.bilibili.com";
const PASSPORT_BASE: &str = "https://passport.bilibili.com";

/// 模拟浏览器 UA（B 站接口对陌生 UA 风控较严）。
pub const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

#[derive(Debug, thiserror::Error)]
pub enum BiliError {
    #[error("B 站接口返回错误 code={code}: {message}")]
    Api { code: i64, message: String },
    #[error("未登录或登录态失效")]
    NotLoggedIn,
    #[error("无法从输入中解析出 BV 号: {0}")]
    BadInput(String),
    #[error("网络请求失败: {0}")]
    Network(#[from] reqwest::Error),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 处理失败: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, BiliError>;

/// B 站客户端：持有 HTTP 会话、登录态与 WBI 签名器。
pub struct BiliClient {
    http: reqwest::Client,
    cred: RwLock<Credential>,
    wbi: wbi::WbiKeys,
    store_path: Option<PathBuf>,
}

impl BiliClient {
    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        Ok(Self {
            http,
            cred: RwLock::new(Credential::default()),
            wbi: wbi::WbiKeys::new(),
            store_path: None,
        })
    }

    /// 设置登录态持久化路径；构造时尝试加载已存登录态。
    pub fn with_store(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if let Ok(cred) = Credential::load(&path) {
            if !cred.is_empty() {
                *self.cred.write().unwrap() = cred;
            }
        }
        self.store_path = Some(path);
        self
    }

    /// 覆盖登录态（内存 + 可选落盘）。
    pub fn set_credential(&self, cred: Credential) -> Result<()> {
        *self.cred.write().unwrap() = cred.clone();
        if let Some(path) = &self.store_path {
            cred.save(path)?;
        }
        Ok(())
    }

    /// 清除登录态（内存 + 可选落盘）。
    pub fn clear_credential(&self) -> Result<()> {
        *self.cred.write().unwrap() = Credential::default();
        if let Some(path) = &self.store_path {
            Credential::default().save(path)?;
        }
        Ok(())
    }

    pub fn credential(&self) -> Credential {
        self.cred.read().unwrap().clone()
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub(crate) fn wbi(&self) -> &wbi::WbiKeys {
        &self.wbi
    }

    /// 组装带登录态 Cookie 头的 GET 请求。
    pub(crate) fn authed_get(&self, url: &str) -> reqwest::RequestBuilder {
        let cred = self.cred.read().unwrap();
        let mut req = self
            .http
            .get(url)
            .header("Referer", "https://www.bilibili.com/");
        if let Some(cookie) = cred.header_value() {
            req = req.header("Cookie", cookie);
        }
        req
    }

    /// 用 nav 接口校验登录态是否真实有效。
    pub async fn is_logged_in(&self) -> bool {
        #[derive(serde::Deserialize)]
        struct Nav {
            #[serde(default)]
            data: NavData,
        }
        #[derive(serde::Deserialize, Default)]
        struct NavData {
            #[serde(default, rename = "isLogin")]
            is_login: bool,
        }
        let resp = self
            .authed_get(&format!("{API_BASE}/x/web-interface/nav"))
            .send()
            .await;
        match resp {
            Ok(r) => r.json::<Nav>().await.map(|n| n.data.is_login).unwrap_or(false),
            Err(_) => false,
        }
    }

    /// 从任意输入解析 BV 号：裸 BV 号、视频页链接；b23.tv 短链自动跟随重定向。
    pub async fn resolve_bvid(&self, input: &str) -> Result<String> {
        if let Some(bvid) = parse_bvid(input) {
            return Ok(bvid);
        }
        if input.contains("b23.tv") {
            let url = input.trim().to_string();
            let resp = self
                .http
                .get(&url)
                .header("Referer", "https://www.bilibili.com/")
                .send()
                .await?;
            let final_url = resp.url().to_string();
            if let Some(bvid) = parse_bvid(&final_url) {
                return Ok(bvid);
            }
        }
        Err(BiliError::BadInput(input.to_string()))
    }

    // ---- 扫码登录 ----

    /// 申请登录二维码。
    pub async fn qrcode_generate(&self) -> Result<qrcode::QrGenerate> {
        qrcode::generate(&self.http).await
    }

    /// 轮询扫码状态；成功时返回登录态（调用方应 `set_credential` 保存）。
    pub async fn qrcode_poll(&self, qrcode_key: &str) -> Result<qrcode::QrPoll> {
        qrcode::poll(&self.http, qrcode_key).await
    }

    // ---- 视频与字幕 ----

    /// 视频信息（标题 / 首个 cid / 分 P）。
    pub async fn video_view(&self, bvid: &str) -> Result<video::VideoInfo> {
        video::video_view(self, bvid).await
    }

    /// 字幕列表（需登录态，否则为空）。
    pub async fn subtitle_tracks(&self, bvid: &str, cid: u64) -> Result<Vec<video::SubtitleTrack>> {
        video::subtitle_tracks(self, bvid, cid).await
    }

    /// 下载并解析字幕内容。
    pub async fn subtitle_content(&self, url: &str) -> Result<Vec<video::SubtitleLine>> {
        video::subtitle_content(self, url).await
    }
}

/// 从文本中提取 BV 号（BV + 10 位字母数字）。
pub fn parse_bvid(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.len() < 12 {
        return None;
    }
    for i in 0..=bytes.len() - 12 {
        if &text[i..i + 2] == "BV" {
            let candidate = &text[i..i + 12];
            if candidate[2..].bytes().all(|b| b.is_ascii_alphanumeric()) {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bvid_from_url_and_bare() {
        assert_eq!(
            parse_bvid("https://www.bilibili.com/video/BV1GJ411x7h7/?p=1"),
            Some("BV1GJ411x7h7".into())
        );
        assert_eq!(parse_bvid("BV1GJ411x7h7"), Some("BV1GJ411x7h7".into()));
        assert_eq!(parse_bvid("https://example.com/nothing"), None);
    }
}
