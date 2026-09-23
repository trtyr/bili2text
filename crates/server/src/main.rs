//! toolbox 服务入口。

use std::sync::Arc;

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));

    // 平台能力初始化：B 站客户端（含登录态持久化）+ 任务存储
    let client = Arc::new(
        bili_client::BiliClient::new()
            .expect("failed to build bili client")
            .with_store("data/credential.json"),
    );
    let tasks = Arc::new(
        platform_core::tasks::TaskStore::open("data/tasks.db")
            .expect("failed to open task store"),
    );

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));

    println!("toolbox server listening on http://{addr}");
    axum::serve(listener, server::app(client, tasks))
        .await
        .expect("server error");
}
