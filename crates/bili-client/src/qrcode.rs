//! 扫码登录：web 端二维码申请与轮询。
//!
//! - `GET /x/passport-login/web/qrcode/generate` → `{url, qrcode_key}`（180s 有效）
//! - `GET /x/passport-login/web/qrcode/poll?qrcode_key=` → data.code：
//!   0 成功 / 86038 已失效 / 86090 已扫未确认 / 86101 未扫码；
//!   成功时 Set-Cookie 下发 SESSDATA / bili_jct / DedeUserID 等。

use serde::Deserialize;

use crate::credential::Credential;
use crate::{BiliError, PASSPORT_BASE, Result};

pub const POLL_SUCCESS: i64 = 0;
pub const POLL_EXPIRED: i64 = 86038;
pub const POLL_SCANNED: i64 = 86090;
pub const POLL_WAITING: i64 = 86101;

/// 申请二维码的返回。
#[derive(Debug, Clone)]
pub struct QrGenerate {
    /// 二维码内容（登录页 URL），前端据此渲染二维码。
    pub url: String,
    /// 轮询用密钥（32 字符）。
    pub qrcode_key: String,
}

/// 一次轮询的返回。
#[derive(Debug, Clone)]
pub struct QrPoll {
    /// B 站状态码（0/86038/86090/86101）。
    pub code: i64,
    pub message: String,
    /// code==0 时的登录态。
    pub credential: Option<Credential>,
}

#[derive(Deserialize)]
struct Envelope {
    code: i64,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: serde_json::Value,
}

/// 申请登录二维码。
pub async fn generate(http: &reqwest::Client) -> Result<QrGenerate> {
    let env: Envelope = http
        .get(format!("{PASSPORT_BASE}/x/passport-login/web/qrcode/generate"))
        .send()
        .await?
        .json()
        .await?;
    if env.code != 0 {
        return Err(BiliError::Api {
            code: env.code,
            message: env.message,
        });
    }
    let url = env
        .data
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let qrcode_key = env
        .data
        .get("qrcode_key")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if url.is_empty() || qrcode_key.is_empty() {
        return Err(BiliError::Api {
            code: -1,
            message: "二维码接口返回缺少 url/qrcode_key".into(),
        });
    }
    Ok(QrGenerate { url, qrcode_key })
}

/// 轮询扫码状态；成功时从 Set-Cookie 抓取登录态。
pub async fn poll(http: &reqwest::Client, qrcode_key: &str) -> Result<QrPoll> {
    let resp = http
        .get(format!("{PASSPORT_BASE}/x/passport-login/web/qrcode/poll"))
        .query(&[("qrcode_key", qrcode_key)])
        .send()
        .await?;

    // 必须在读 body 之前抓 Set-Cookie（json() 会消费响应）
    let cookies: Vec<(String, String)> = resp
        .cookies()
        .map(|c| (c.name().to_string(), c.value().to_string()))
        .collect();

    let env: Envelope = resp.json().await?;
    let code = env
        .data
        .get("code")
        .and_then(|v| v.as_i64())
        .unwrap_or(env.code);
    let message = env
        .data
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or(&env.message)
        .to_string();

    let credential = if code == POLL_SUCCESS {
        let cred = Credential::from_pairs(cookies);
        (cred.header_value().is_some()).then_some(cred)
    } else {
        None
    };

    Ok(QrPoll {
        code,
        message,
        credential,
    })
}
