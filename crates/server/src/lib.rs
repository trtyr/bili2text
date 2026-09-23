//! HTTP 服务：组装工具注册表与平台级端点。
//!
//! - `GET /api/health`       — 健康检查
//! - `GET /api/tools`        — 工具目录（注册表自动生成）
//! - `/api/tools/{id}/*`     — 各工具自有路由（nest 挂载）

use axum::{Json, Router, http::StatusCode, routing::get};
use platform_core::{Tool, ToolInfo};
use std::sync::Arc;
use tool_bili2text::Bili2TextTool;

/// 已注册的工具集合。未来新增工具：实现 `Tool` 后在这里加一行。
pub fn registry() -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(Bili2TextTool)]
}

async fn health() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

async fn list_tools() -> Json<Vec<ToolInfo>> {
    Json(registry().iter().map(|t| ToolInfo::of(t.as_ref())).collect())
}

/// 组装完整应用路由。
pub fn app() -> Router {
    let tools = registry();

    let mut api = Router::new()
        .route("/health", get(health))
        .route("/tools", get(list_tools));

    for tool in &tools {
        let path = format!("/tools/{}", tool.id());
        api = api.nest(&path, tool.router());
    }

    Router::new()
        .nest("/api", api)
        .layer(tower_http::cors::CorsLayer::permissive())
}
