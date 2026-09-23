//! B 站视频转文字工具（完整子系统）。
//!
//! 功能面：
//! - 扫码登录（`/auth/*`）：登录是本工具内部能力，路由挂 `/api/tools/bili2text/auth/*`
//! - 字幕提取（`/tracks`、`/extract`）：贴链接 → 字幕列表（CC/AI、多语言）→ 文本/SRT
//!
//! ASR 链路（SenseVoice 本地转写）在后续任务接入。

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use bili_client::{BiliClient, BiliError};
use platform_core::Tool;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// 工具实例：持有共享的 B 站客户端（登录态在其中管理）。
pub struct Bili2TextTool {
    client: Arc<BiliClient>,
}

impl Bili2TextTool {
    pub fn new(client: Arc<BiliClient>) -> Self {
        Self { client }
    }
}

#[derive(Debug, Deserialize)]
pub struct VideoReq {
    /// BV 号、视频页链接或 b23.tv 短链。
    pub input: String,
    /// 可选：指定字幕语言（lan，如 "zh-Hans" / "ai-zh"）；缺省自动挑选。
    pub lang: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TrackOut {
    pub lan: String,
    pub lan_doc: String,
    pub is_ai: bool,
}

type ApiError = (StatusCode, Json<Value>);

fn err_resp(e: BiliError) -> ApiError {
    let (status, code) = match &e {
        BiliError::NotLoggedIn => (StatusCode::UNAUTHORIZED, "login_required"),
        BiliError::BadInput(_) => (StatusCode::BAD_REQUEST, "bad_input"),
        BiliError::Api { .. } => (StatusCode::BAD_GATEWAY, "bili_api_error"),
        BiliError::Network(_) | BiliError::Io(_) | BiliError::Json(_) => {
            (StatusCode::BAD_GATEWAY, "network_error")
        }
    };
    (
        status,
        Json(json!({ "error": code, "message": e.to_string() })),
    )
}

// ---- 扫码登录（工具内部能力） ----

/// `POST /api/tools/bili2text/auth/qrcode` — 申请登录二维码。
async fn auth_qrcode(
    State(client): State<Arc<BiliClient>>,
) -> Result<Json<Value>, ApiError> {
    let qr = client.qrcode_generate().await.map_err(err_resp)?;
    Ok(Json(json!({
        "qrcode_key": qr.qrcode_key,
        "qr_content": qr.url,
        "expires_in_secs": 180,
    })))
}

/// `GET /api/tools/bili2text/auth/qrcode/poll?qrcode_key=` — 轮询扫码状态。
async fn auth_poll(
    State(client): State<Arc<BiliClient>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let Some(key) = params.get("qrcode_key") else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing_qrcode_key" })),
        ));
    };
    let poll = client.qrcode_poll(key).await.map_err(err_resp)?;
    let mut logged_in = false;
    if poll.code == bili_client::qrcode::POLL_SUCCESS {
        if let Some(cred) = poll.credential {
            client.set_credential(cred).map_err(err_resp)?;
            logged_in = true;
        }
    }
    Ok(Json(json!({
        "code": poll.code,
        "message": poll.message,
        "logged_in": logged_in,
        // 状态语义：86101 未扫 / 86090 已扫未确认 / 86038 已失效 / 0 成功
        "status": match poll.code {
            bili_client::qrcode::POLL_SUCCESS => "success",
            bili_client::qrcode::POLL_SCANNED => "scanned",
            bili_client::qrcode::POLL_EXPIRED => "expired",
            _ => "waiting",
        },
    })))
}

/// `GET /api/tools/bili2text/auth/status` — 登录态状态
/// （has_credential 为本地是否有存档；logged_in 真实请求 B 站校验）。
async fn auth_status(State(client): State<Arc<BiliClient>>) -> Json<Value> {
    let has = client.credential().header_value().is_some();
    let logged_in = if has { client.is_logged_in().await } else { false };
    Json(json!({ "has_credential": has, "logged_in": logged_in }))
}

/// `DELETE /api/tools/bili2text/auth` — 登出（清除本地登录态）。
async fn auth_logout(State(client): State<Arc<BiliClient>>) -> Result<Json<Value>, ApiError> {
    client.clear_credential().map_err(err_resp)?;
    Ok(Json(json!({ "logged_out": true })))
}

// ---- 字幕提取 ----

/// `POST /api/tools/bili2text/tracks` — 列出视频可用字幕轨。
async fn tracks(
    State(client): State<Arc<BiliClient>>,
    Json(req): Json<VideoReq>,
) -> Result<Json<Value>, ApiError> {
    let bvid = client.resolve_bvid(&req.input).await.map_err(err_resp)?;
    let info = client.video_view(&bvid).await.map_err(err_resp)?;
    let list = client
        .subtitle_tracks(&bvid, info.cid)
        .await
        .map_err(err_resp)?;

    let tracks: Vec<TrackOut> = list
        .iter()
        .map(|t| TrackOut {
            lan: t.lan.clone(),
            lan_doc: t.lan_doc.clone(),
            is_ai: t.is_ai,
        })
        .collect();

    Ok(Json(json!({
        "bvid": bvid,
        "title": info.title,
        "duration_secs": info.duration_secs,
        "tracks": tracks,
        "logged_in": client.credential().header_value().is_some(),
    })))
}

/// 挑选字幕轨：显式 lang > 非 AI 中文 > 非 AI 第一条 > AI 中文 > 第一条。
fn pick_track<'a>(
    tracks: &'a [bili_client::video::SubtitleTrack],
    lang: Option<&str>,
) -> Option<&'a bili_client::video::SubtitleTrack> {
    if let Some(want) = lang {
        return tracks.iter().find(|t| t.lan == want);
    }
    tracks
        .iter()
        .find(|t| !t.is_ai && t.lan.starts_with("zh"))
        .or_else(|| tracks.iter().find(|t| !t.is_ai))
        .or_else(|| tracks.iter().find(|t| t.is_ai && t.lan.starts_with("zh")))
        .or_else(|| tracks.first())
}

fn no_subtitle(bvid: &str, title: &str) -> Json<Value> {
    Json(json!({
        "error": "no_subtitle",
        "message": "该视频没有可用字幕（AI 字幕需要登录，且并非所有视频都生成过）",
        "bvid": bvid,
        "title": title,
    }))
}

/// `POST /api/tools/bili2text/extract` — 提取字幕为文本（含 SRT）。
async fn extract(
    State(client): State<Arc<BiliClient>>,
    Json(req): Json<VideoReq>,
) -> Result<Json<Value>, ApiError> {
    let bvid = client.resolve_bvid(&req.input).await.map_err(err_resp)?;
    let info = client.video_view(&bvid).await.map_err(err_resp)?;
    let list = client
        .subtitle_tracks(&bvid, info.cid)
        .await
        .map_err(err_resp)?;

    if list.is_empty() {
        return Ok(no_subtitle(&bvid, &info.title));
    }

    let track = match pick_track(&list, req.lang.as_deref()) {
        Some(t) => t.clone(),
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "lang_not_found",
                    "message": "指定的字幕语言不存在",
                    "available": list.iter().map(|t| t.lan.clone()).collect::<Vec<_>>(),
                })),
            ));
        }
    };

    let lines = client
        .subtitle_content(&track.url)
        .await
        .map_err(err_resp)?;

    let text = lines
        .iter()
        .map(|l| l.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let srt = to_srt(&lines);

    Ok(Json(json!({
        "bvid": bvid,
        "title": info.title,
        "duration_secs": info.duration_secs,
        "subtitle": {
            "lan": track.lan,
            "lan_doc": track.lan_doc,
            "is_ai": track.is_ai,
        },
        "lines_count": lines.len(),
        "text": text,
        "srt": srt,
    })))
}

/// 字幕行转 SRT 格式（00:00:00,000 --> ...）。
fn to_srt(lines: &[bili_client::video::SubtitleLine]) -> String {
    fn stamp(secs: f64) -> String {
        let total_ms = (secs.max(0.0) * 1000.0).round() as u64;
        let h = total_ms / 3_600_000;
        let m = (total_ms % 3_600_000) / 60_000;
        let s = (total_ms % 60_000) / 1000;
        let ms = total_ms % 1000;
        format!("{h:02}:{m:02}:{s:02},{ms:03}")
    }
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            format!(
                "{}\n{} --> {}\n{}\n",
                i + 1,
                stamp(l.from),
                stamp(l.to),
                l.content
            )
        })
        .collect()
}

/// `GET /api/tools/bili2text/info` — 工具能力自述。
async fn info() -> Json<Value> {
    Json(json!({
        "id": "bili2text",
        "name": "B站视频转文字",
        "modes": ["subtitle", "asr"],
        "subtitle": {
            "sources": ["cc", "ai"],
            "needs_login": true,
            "status": "available",
        },
        "asr": {
            "engine": "sense-voice (sherpa-onnx)",
            "languages": ["zh", "en", "ja", "ko", "yue"],
            "planned": true,
        },
        "endpoints": ["POST /tracks", "POST /extract", "auth: qrcode/poll/status/logout"],
    }))
}

impl Tool for Bili2TextTool {
    fn id(&self) -> &'static str {
        "bili2text"
    }

    fn name(&self) -> &'static str {
        "B站视频转文字"
    }

    fn description(&self) -> &'static str {
        "提取 B 站视频字幕（CC/AI），无字幕时本地模型转写"
    }

    fn router(&self) -> Router {
        Router::new()
            .route("/info", get(info))
            .route("/tracks", post(tracks))
            .route("/extract", post(extract))
            .route("/auth/qrcode", post(auth_qrcode))
            .route("/auth/qrcode/poll", get(auth_poll))
            .route("/auth/status", get(auth_status))
            .route("/auth", delete(auth_logout))
            .with_state(self.client.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_format() {
        let lines = vec![
            bili_client::video::SubtitleLine {
                from: 0.0,
                to: 1.5,
                content: "你好".into(),
            },
            bili_client::video::SubtitleLine {
                from: 62.25,
                to: 65.0,
                content: "世界".into(),
            },
        ];
        let srt = to_srt(&lines);
        assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:01,500\n你好\n"));
        assert!(srt.contains("2\n00:01:02,250 --> 00:01:05,000\n世界"));
    }

    #[test]
    fn pick_prefers_native_zh() {
        let mk = |lan: &str, ai: bool| bili_client::video::SubtitleTrack {
            id: String::new(),
            lan: lan.into(),
            lan_doc: String::new(),
            is_ai: ai,
            url: String::new(),
        };
        // 有官方字幕（即使非中文）也不选 AI 字幕
        let tracks = vec![mk("en-US", false), mk("ai-zh", true)];
        assert_eq!(pick_track(&tracks, None).unwrap().lan, "en-US");

        // 中文官方 > 英文官方 > AI 中文 > AI 任意
        let tracks = vec![mk("ai-zh", true), mk("zh-Hans", false)];
        assert_eq!(pick_track(&tracks, None).unwrap().lan, "zh-Hans");

        let tracks = vec![mk("ai-zh", true)];
        assert_eq!(pick_track(&tracks, None).unwrap().lan, "ai-zh");
    }
}
