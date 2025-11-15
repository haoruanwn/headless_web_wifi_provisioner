//! 嵌入式 WiFi 配网工具核心库
//! 
//! 这个库提供了 `run_provisioner()` 函数，封装了整个配网流程：
//! 1. 初始化 wpa_supplicant 并扫描可用网络
//! 2. 启动 AP 热点以接收用户配置
//! 3. 运行 Web 服务器等待用户输入
//! 4. 在获得用户输入后连接到目标网络

use anyhow::Result;
use std::sync::Arc;

// 声明所有模块
pub mod backend;
pub mod config;
pub mod embed;
pub mod structs;
pub mod traits;
mod web_server;

#[cfg(feature = "audio")]
pub mod audio;

// 导入核心后端
use backend::WpaCtrlBackend;

/// 核心配网流程：扫描网络、启动 AP、运行 Web 服务器
/// 
/// 这个函数是整个应用的核心逻辑入口。它会：
/// 1. 创建 WpaCtrlBackend 实例
/// 2. 执行初始扫描并启动 AP
/// 3. 启动 Web 服务器等待用户配置
/// 4. 在用户选择网络并输入密码时自动连接
pub async fn run_provisioner() -> Result<()> {
    tracing::info!("🚀 Starting provisioner with wpa_ctrl backend...");

    // 创建后端实例
    let backend = Arc::new(WpaCtrlBackend::new()?);

    // 执行 TDM 启动序列：扫描 -> 启动 AP
    tracing::info!("📡 Executing initial scan and starting AP...");
    let initial_networks = match backend.setup_and_scan().await {
        Ok(networks) => {
            tracing::info!(
                "✅ Initial scan complete, found {} networks. AP started.",
                networks.len()
            );
            networks
        }
        Err(e) => {
            // 将错误冒泡到调用者
            tracing::error!("❌ Failed to scan or start AP: {}", e);
            return Err(e);
        }
    };

    // 启动 Web 服务器
    if let Err(e) = web_server::run_server(backend, initial_networks).await {
        // 将错误冒泡到调用者
        tracing::error!("❌ Web server failed: {}", e);
        return Err(e);
    }

    tracing::info!("🛑 Shutting down.");
    Ok(())
}
