//! HTTP 服务：平台层。
//!
//! 职责分层（平台能力 vs 工具编排）：
//! - **平台端点**：`/api/health`、`/api/tools`（工具目录）
//! - **平台账号服务**：`/api/platform/bili/*` —— B 站账号的扫码登录/状态/登出。
//!   登录态是平台的资产（存 `data/credential.json`），由平台统一持有和管理，
//!   所有工具共享；工具不own登录流程。
//! - **工具挂载**：各工具的业务路由 nest 在 `/api/tools/{id}` 下，工具只做编排。
//!
//! 能力库（如 bili-client）以 crate 形式供工具调用；未来的下载器、ASR 引擎
//! 同样是平台能力，落成独立 crate。

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use bili_client::BiliClient;
use platform_core::{Tool, ToolInfo};
use serde_json::{Value, json};
use tool_bili2text::Bili2TextTool;

/// 已注册的工具集合。未来新增工具：实现 `Tool` 后在这里加一行。
pub fn registry(client: Arc<BiliClient>) -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(Bili2TextTool::new(client.clone()))]
}

async fn health() -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

async fn list_tools(State(client): State<Arc<BiliClient>>) -> Json<Vec<ToolInfo>> {
    Json(
        registry(client)
            .iter()
            .map(|t| ToolInfo::of(t.as_ref()))
            .collect(),
    )
}

// ---- 平台账号服务：B 站账号（登录态为平台资产，工具共享） ----

/// `POST /api/platform/bili/qrcode` — 申请登录二维码。
async fn bili_qrcode(
    State(client): State<Arc<BiliClient>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let qr = client.qrcode_generate().await.map_err(platform_err)?;
    Ok(Json(json!({
        "qrcode_key": qr.qrcode_key,
        "qr_content": qr.url,
        "expires_in_secs": 180,
    })))
}

/// `GET /api/platform/bili/qrcode/poll?qrcode_key=` — 轮询扫码状态；
/// 成功时登录态落库（内存 + data/credential.json）。
async fn bili_qrcode_poll(
    State(client): State<Arc<BiliClient>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let Some(key) = params.get("qrcode_key") else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing_qrcode_key" })),
        ));
    };
    let poll = client.qrcode_poll(key).await.map_err(platform_err)?;
    let mut logged_in = false;
    if poll.code == bili_client::qrcode::POLL_SUCCESS {
        if let Some(cred) = poll.credential {
            client.set_credential(cred).map_err(platform_err)?;
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

/// `GET /api/platform/bili/status` — 登录态状态
/// （has_credential 为本地是否有存档；logged_in 真实请求 B 站校验有效性）。
async fn bili_status(State(client): State<Arc<BiliClient>>) -> Json<Value> {
    let has = client.credential().header_value().is_some();
    let logged_in = if has { client.is_logged_in().await } else { false };
    Json(json!({ "has_credential": has, "logged_in": logged_in }))
}

/// `DELETE /api/platform/bili` — 登出（清除平台持有的登录态）。
async fn bili_logout(
    State(client): State<Arc<BiliClient>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    client.clear_credential().map_err(platform_err)?;
    Ok(Json(json!({ "logged_out": true })))
}

fn platform_err(e: bili_client::BiliError) -> (StatusCode, Json<Value>) {
    let status = match &e {
        bili_client::BiliError::BadInput(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::BAD_GATEWAY,
    };
    (
        status,
        Json(json!({ "error": "platform_error", "message": e.to_string() })),
    )
}

/// 组装完整应用路由。
pub fn app(client: Arc<BiliClient>) -> Router {
    let mut api = Router::new()
        .route("/health", get(health))
        .route("/tools", get(list_tools))
        .route("/platform/bili/qrcode", post(bili_qrcode))
        .route("/platform/bili/qrcode/poll", get(bili_qrcode_poll))
        .route("/platform/bili/status", get(bili_status))
        .route("/platform/bili", delete(bili_logout))
        .with_state(client.clone());

    for tool in registry(client) {
        let path = format!("/tools/{}", tool.id());
        api = api.nest(&path, tool.router());
    }

    Router::new()
        .nest("/api", api)
        .layer(tower_http::cors::CorsLayer::permissive())
}
