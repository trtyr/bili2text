//! HTTP 服务：平台层。只做两件事——
//! 1. 平台端点：健康检查、工具目录；
//! 2. 把各工具的完整路由（含其内部功能如登录）nest 到 `/api/tools/{id}` 下。
//!
//! 平台不感知工具内部功能（如扫码登录）——那是各工具子系统自己的事。

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::get,
};
use bili_client::BiliClient;
use platform_core::{Tool, ToolInfo};
use serde_json::{Value, json};

/// 已注册的工具集合。未来新增工具：实现 `Tool` 后在这里加一行。
pub fn registry(client: Arc<BiliClient>) -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(tool_bili2text::Bili2TextTool::new(client))]
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

/// 组装完整应用路由。
pub fn app(client: Arc<BiliClient>) -> Router {
    let mut api = Router::new()
        .route("/health", get(health))
        .route("/tools", get(list_tools))
        .with_state(client.clone());

    for tool in registry(client) {
        let path = format!("/tools/{}", tool.id());
        api = api.nest(&path, tool.router());
    }

    Router::new()
        .nest("/api", api)
        .layer(tower_http::cors::CorsLayer::permissive())
}
