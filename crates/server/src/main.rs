//! toolbox 服务入口。

use std::sync::Arc;

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));

    // 登录态持久化到 data/credential.json（相对 cwd；data/ 已在 .gitignore）
    let client = Arc::new(
        bili_client::BiliClient::new()
            .expect("failed to build bili client")
            .with_store("data/credential.json"),
    );

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));

    println!("toolbox server listening on http://{addr}");
    axum::serve(listener, server::app(client))
        .await
        .expect("server error");
}
