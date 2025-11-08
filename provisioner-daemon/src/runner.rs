use provisioner_core::{
    web_server,
    traits::{UiAssetProvider, TdmBackend, ConcurrentBackend},
};
use std::sync::Arc;

// 1. Define a public Enum that holds a type-erased Trait object.
pub enum BackendRunner {
    Tdm(Arc<dyn TdmBackend + Send + Sync + 'static>),
    Concurrent(Arc<dyn ConcurrentBackend + Send + Sync + 'static>),
}

// 2. Remove all cfg blocks from runner.rs.
//    `run_provisioning_server` now accepts the Enum.
pub async fn run_provisioning_server<F>(
    frontend: Arc<F>,
    backend_runner: BackendRunner, // Accept the Enum
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