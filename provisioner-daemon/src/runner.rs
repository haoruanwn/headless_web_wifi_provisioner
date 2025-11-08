use provisioner_core::{
    traits::{ConcurrentBackend, TdmBackend, UiAssetProvider},
    web_server,
};
use std::sync::Arc;

// 不同的后端能力通过该枚举进行区分和传递
pub enum BackendRunner {
    Tdm(Arc<dyn TdmBackend + Send + Sync + 'static>),
    Concurrent(Arc<dyn ConcurrentBackend + Send + Sync + 'static>),
}

// 通过BackendRunner枚举来区分不同后端能力，来调用不同的服务器启动逻辑
pub async fn run_provisioning_server<F>(
    frontend: Arc<F>,
    backend_runner: BackendRunner, // 接受枚举类型
) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    match backend_runner {
        BackendRunner::Tdm(backend) => {
            println!("📡 Runner: Starting TDM server...");
            web_server::start_tdm_server(backend, frontend).await??;
        }
        BackendRunner::Concurrent(backend) => {
            println!("📡 Runner: Starting Concurrent server...");
            web_server::start_concurrent_server(backend, frontend).await??;
        }
    }
    Ok(())
}
