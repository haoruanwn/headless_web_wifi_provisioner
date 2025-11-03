use provisioner_core::{
    traits::{ProvisioningBackend, UiAssetProvider},
    web_server::start_server, // 假设 `start_server` 在这个模块
};
use std::sync::Arc;

// --- 1. 选择后端实现 ---
#[cfg(feature = "backend_dbus")]
fn get_backend() -> Arc<dyn ProvisioningBackend> {
    println!("🚀 Using D-Bus Backend");
    // 导入并创建 DbusBackend
    // use provisioner_core::backends::dbus_backend::DbusBackend;
    // Arc::new(DbusBackend::new()) 
    
    // (暂用 Mock 替代，直到 DbusBackend 实现)
    use provisioner_core::backends::mock_backend::MockBackend;
    Arc::new(MockBackend::new())
}

#[cfg(feature = "backend_mock")]
fn get_backend() -> Arc<dyn ProvisioningBackend> {
    println!("🚀 Using Mock Backend");
    use provisioner_core::backends::mock_backend::MockBackend;
    Arc::new(MockBackend::new())
}

// 如果没有选择任何后端，编译失败
#[cfg(not(any(feature = "backend_dbus", feature = "backend_mock")))]
compile_error!("No backend feature selected. Please choose one, e.g., --features provisioner-daemon/backend_dbus");


// --- 2. 选择前端实现 ---
#[cfg(feature = "frontend_embed")]
fn get_frontend() -> Arc<dyn UiAssetProvider> {
    println!("🚀 Using Embedded UI Frontend");
    use provisioner_core::frontends::embed_frontend::EmbeddedFrontend;
    Arc::new(EmbeddedFrontend)
}

#[cfg(feature = "frontend_disk")]
fn get_frontend() -> Arc<dyn UiAssetProvider> {
    println!("🚀 Using Disk UI Frontend (Debug Mode)");
    use provisioner_core::frontends::disk_frontend::DiskFrontend;
    Arc::new(DiskFrontend)
}

// 如果没有选择任何前端，编译失败
#[cfg(not(any(feature = "frontend_embed", feature = "frontend_disk")))]
compile_error!("No frontend feature selected. Please choose one, e.g., --features provisioner-daemon/frontend_embed");


// --- 3. 启动服务器 ---
#[tokio::main]
async fn main() {
    // 初始化日志
    // tracing_subscriber::fmt::init();
    
    // 1. 基于特性，在编译时决定实例化哪个后端和前端
    let backend = get_backend();
    let frontend = get_frontend();
    
    // 2. 启动 DHCP 和 DNS 服务 (在 `provisioner-core` 中实现)
    // provisioner_core::dhcp::start_dhcp_server().await;
    // provisioner_core::dns::start_dns_server().await;

    // 3. 启动泛型的 Web 服务器，将实现"注入"
    println!("Starting web server...");
    start_server(backend, frontend).await;
}