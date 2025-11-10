mod backend;
mod config;
mod structs;
mod web_server;

use anyhow::Result;
use backend::WpaDbusBackend;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("🚀 Starting simple-provisioner-wpadbus...");

    // 创建后端实例
    let backend = Arc::new(WpaDbusBackend::new()?);

    // 执行 TDM 启动序列：扫描 -> 启动 AP
    tracing::info!("📡 Executing initial D-Bus scan and starting AP...");
    let initial_networks = match backend.setup_and_scan().await {
        Ok(networks) => {
            tracing::info!(
                "✅ Initial scan complete, found {} networks. AP started.",
                networks.len()
            );
            networks
        }
        Err(e) => {
            tracing::error!("❌ Failed to scan or start AP: {}. Exiting.", e);
            return Err(e);
        }
    };

    // 启动 Web 服务器
    if let Err(e) = web_server::run_server(backend, initial_networks).await {
        tracing::error!("❌ Web server failed: {}", e);
    }

    tracing::info!("🛑 Shutting down.");
    Ok(())
}
