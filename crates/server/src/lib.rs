//! HTTP 服务：组装工具注册表、平台级端点与 B 站登录。
//!
//! 平台端点：
//! - `GET  /api/health` — 健康检查
//! - `GET  /api/tools`  — 工具目录（注册表自动生成）
//! - `POST /api/auth/bili/qrcode`            — 申请登录二维码
//! - `GET  /api/auth/bili/qrcode/poll`       — 轮询扫码状态（成功即落登录态）
//! - `GET  /api/auth/bili/status`            — 登录态状态（含真实有效性校验）
//! - `DELETE /api/auth/bili`                 — 登出
//! 工具路由挂载在 `/api/tools/{id}/*`。

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

// ---- B 站登录（平台级：登录态为所有工具共享） ----

/// `POST /api/auth/bili/qrcode` — 申请登录二维码。
async fn bili_qrcode(
    State(client): State<Arc<BiliClient>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let qr = client.qrcode_generate().await.map_err(bili_err)?;
    Ok(Json(json!({
        "qrcode_key": qr.qrcode_key,
        "qr_content": qr.url,
        "expires_in_secs": 180,
    })))
}

/// `GET /api/auth/bili/qrcode/poll?qrcode_key=` — 轮询扫码状态。
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
    let poll = client.qrcode_poll(key).await.map_err(bili_err)?;
    let mut logged_in = false;
    if poll.code == bili_client::qrcode::POLL_SUCCESS {
        if let Some(cred) = poll.credential {
            client.set_credential(cred).map_err(bili_err)?;
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

/// `GET /api/auth/bili/status` — 登录态状态（has_credential 为本地是否有存档；
/// logged_in 会真实请求 B 站校验有效性）。
async fn bili_status(State(client): State<Arc<BiliClient>>) -> Json<Value> {
    let has = client.credential().header_value().is_some();
    let logged_in = if has { client.is_logged_in().await } else { false };
    Json(json!({ "has_credential": has, "logged_in": logged_in }))
}

/// `DELETE /api/auth/bili` — 登出（清除本地登录态）。
async fn bili_logout(
    State(client): State<Arc<BiliClient>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    client.clear_credential().map_err(bili_err)?;
    Ok(Json(json!({ "logged_out": true })))
}

fn bili_err(e: bili_client::BiliError) -> (StatusCode, Json<Value>) {
    let status = match &e {
        bili_client::BiliError::BadInput(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::BAD_GATEWAY,
    };
    (
        status,
        Json(json!({ "error": "bili_error", "message": e.to_string() })),
    )
}

/// 组装完整应用路由。
pub fn app(client: Arc<BiliClient>) -> Router {
    let mut api = Router::new()
        .route("/health", get(health))
        .route("/tools", get(list_tools))
        .route("/auth/bili/qrcode", post(bili_qrcode))
        .route("/auth/bili/qrcode/poll", get(bili_qrcode_poll))
        .route("/auth/bili/status", get(bili_status))
        .route("/auth/bili", delete(bili_logout))
        .with_state(client.clone());

    for tool in registry(client) {
        let path = format!("/tools/{}", tool.id());
        api = api.nest(&path, tool.router());
    }

    Router::new()
        .nest("/api", api)
        .layer(tower_http::cors::CorsLayer::permissive())
}
