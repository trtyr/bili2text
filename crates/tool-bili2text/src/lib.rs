//! B 站视频转文字工具。
//!
//! 两条链路：
//! ① 优先提取站内字幕（CC / AI 字幕，多语言可选）；
//! ② 无字幕时下载音频轨，用 SenseVoice（sherpa-rs 绑定）本地转写。
//!
//! 骨架阶段仅注册工具元信息与 `/info` 端点，转写与登录流程在后续任务接入。

use axum::{Json, Router, routing::get};
use platform_core::Tool;

#[derive(Debug, Default)]
pub struct Bili2TextTool;

/// `GET /api/tools/bili2text/info` — 工具能力自述。
async fn info() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "id": "bili2text",
        "name": "B站视频转文字",
        "modes": ["subtitle", "asr"],
        "subtitle": {
            "sources": ["cc", "ai"],
            "needs_login": true,
        },
        "asr": {
            "engine": "sense-voice (sherpa-onnx)",
            "languages": ["zh", "en", "ja", "ko", "yue"],
            "planned": true,
        },
        "status": "scaffold",
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
        Router::new().route("/info", get(info))
    }
}
