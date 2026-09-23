//! B 站开放接口客户端（骨架）。
//!
//! 规划覆盖（依据 bilibili-API-collect 文档）：
//! - 扫码登录：`GET /x/passport-login/web/qrcode/generate` + `/poll`
//!   （轮询码：86101 未扫 / 86090 已扫未确认 / 86038 失效 / 0 成功，成功即得 SESSDATA）
//! - wbi 签名：`/x/player/wbi/v2` 等接口的公共参数签名
//! - 视频信息与分 P：`GET /x/web-interface/view`
//! - 字幕列表：`GET /x/player/wbi/v2` → `data.subtitle.subtitles[]`
//!   （CC + AI 字幕混排，`ai_type`/`ai_status` 区分；未登录为空数组）
//! - 字幕内容：`aisubtitle.hdslb.com` 的 JSON（`body[{from,to,content}]`）

/// 目标视频引用（骨架占位）。
#[derive(Debug, Clone)]
pub struct VideoRef {
    pub bvid: String,
    pub cid: Option<u64>,
}

impl VideoRef {
    pub fn from_bvid(bvid: impl Into<String>) -> Self {
        Self {
            bvid: bvid.into(),
            cid: None,
        }
    }
}

/// 登录态（骨架占位；后续持久化到 SQLite 并加密存储）。
#[derive(Debug, Clone, Default)]
pub struct Credential {
    pub sessdata: Option<String>,
}

/// B 站客户端（骨架；后续注入 HTTP 客户端 + 登录态 + wbi 签名）。
#[derive(Debug, Default)]
pub struct BiliClient {
    pub credential: Credential,
}
