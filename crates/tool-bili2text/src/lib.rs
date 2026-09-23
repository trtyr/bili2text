//! B 站视频转文字工具（业务编排层）。
//!
//! 这个 crate 只做编排：调用平台能力（bili-client 的接口能力、平台账号服务
//! 持有的登录态、平台任务存储）组合出「视频 → 文本」的业务。
//!
//! - 字幕提取：`POST /tracks`、`POST /extract`（文本 + SRT）
//! - ASR 转写（平台下载能力 + 平台 ASR 能力，规划中）
//!
//! 登录流程不在本工具——那是平台账号服务（`/api/platform/bili/*`）的职责；
//! 每次提取会作为任务记录写入平台任务存储（`/api/platform/tasks`）。

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use bili_client::{BiliClient, BiliError};
use platform_core::Tool;
use platform_core::tasks::{TaskRecord, TaskStore};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// 工具运行时状态：平台能力引用集合。
#[derive(Clone)]
pub struct ToolState {
    pub client: Arc<BiliClient>,
    pub tasks: Arc<TaskStore>,
}

/// 工具实例。
pub struct Bili2TextTool {
    state: ToolState,
}

impl Bili2TextTool {
    pub fn new(client: Arc<BiliClient>, tasks: Arc<TaskStore>) -> Self {
        Self {
            state: ToolState { client, tasks },
        }
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

/// `POST /api/tools/bili2text/tracks` — 列出视频可用字幕轨。
async fn tracks(
    State(state): State<ToolState>,
    Json(req): Json<VideoReq>,
) -> Result<Json<Value>, ApiError> {
    let client = &state.client;
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
/// 每次执行（无论成败）都会作为任务记录写入平台任务存储。
async fn extract(
    State(state): State<ToolState>,
    Json(req): Json<VideoReq>,
) -> Result<Json<Value>, ApiError> {
    let result = run_extract(&state.client, &req).await;

    match result {
        Ok(value) => {
            let task = TaskRecord {
                id: String::new(),
                tool_id: "bili2text".into(),
                kind: "subtitle_extract".into(),
                input: req.input.clone(),
                title: value["title"].as_str().map(String::from),
                bvid: value["bvid"].as_str().map(String::from),
                status: "succeeded".into(),
                error: None,
                lang: value["subtitle"]["lan"].as_str().map(String::from),
                lines_count: value["lines_count"].as_i64(),
                duration_secs: value["duration_secs"].as_i64(),
                result_text: value["text"].as_str().map(String::from),
                result_srt: value["srt"].as_str().map(String::from),
                created_at: 0,
                finished_at: None,
            };
            // 记录失败不影响业务响应（历史是尽力而为）
            if let Ok(id) = state.tasks.record(task) {
                let mut v = value;
                v["task_id"] = json!(id);
                Ok(Json(v))
            } else {
                Ok(Json(value))
            }
        }
        Err((status, Json(mut body))) => {
            let task = TaskRecord {
                id: String::new(),
                tool_id: "bili2text".into(),
                kind: "subtitle_extract".into(),
                input: req.input.clone(),
                title: body["title"].as_str().map(String::from),
                bvid: body["bvid"].as_str().map(String::from),
                status: "failed".into(),
                error: body["error"].as_str().map(String::from),
                lang: req.lang.clone(),
                lines_count: None,
                duration_secs: None,
                result_text: None,
                result_srt: None,
                created_at: 0,
                finished_at: None,
            };
            let _ = state.tasks.record(task);
            Err((status, Json(body)))
        }
    }
}

/// 实际提取逻辑（与任务落库解耦）。
async fn run_extract(client: &BiliClient, req: &VideoReq) -> Result<Value, ApiError> {
    let bvid = client.resolve_bvid(&req.input).await.map_err(err_resp)?;
    let info = client.video_view(&bvid).await.map_err(err_resp)?;
    let list = client
        .subtitle_tracks(&bvid, info.cid)
        .await
        .map_err(err_resp)?;

    if list.is_empty() {
        // 业务性空结果：HTTP 200 + error 字段（前端按错误分支处理）
        return Err((StatusCode::OK, no_subtitle(&bvid, &info.title)));
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

    Ok(json!({
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
    }))
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
            "login_via": "platform: /api/platform/bili",
            "history_via": "platform: /api/platform/tasks",
        },
        "asr": {
            "engine": "sense-voice (sherpa-onnx)",
            "languages": ["zh", "en", "ja", "ko", "yue"],
            "planned": true,
        },
        "endpoints": ["POST /tracks", "POST /extract"],
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
            .with_state(self.state.clone())
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
