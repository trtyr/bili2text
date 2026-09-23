//! 平台核心：工具注册 trait、通用任务模型与平台任务存储。
//!
//! 每个小工具实现 [`Tool`] trait，由 `server` 统一挂载路由并暴露工具目录。
//! 后续新增工具 = 实现该 trait + 在 server 注册表加一行。
//!
//! 模块：
//! - [`tasks`]：平台任务存储（工具执行历史的 SQLite 持久化，平台能力）

pub mod tasks;

use axum::Router;
use serde::Serialize;

/// 一个挂在平台上的小工具。
pub trait Tool: Send + Sync + 'static {
    /// 全局唯一 id，用作 URL 片段：`/api/tools/{id}`。
    fn id(&self) -> &'static str;

    /// 展示名（中文）。
    fn name(&self) -> &'static str;

    /// 一句话描述。
    fn description(&self) -> &'static str;

    /// 工具自有路由，由 server nest 到 `/api/tools/{id}` 下。
    fn router(&self) -> Router;
}

/// 工具目录条目（`GET /api/tools` 的返回项）。
#[derive(Debug, Serialize)]
pub struct ToolInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

impl ToolInfo {
    pub fn of(tool: &dyn Tool) -> Self {
        Self {
            id: tool.id(),
            name: tool.name(),
            description: tool.description(),
        }
    }
}

/// 通用任务状态（后续任务队列复用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}
