use provisioner_core::traits::{PolicyCheck, UiAssetProvider};
use std::sync::Arc;
use crate::runner::BackendRunner; // Import BackendRunner

// Remove all cfg blocks!
#[allow(dead_code)]
pub async fn run<F>(
    frontend: Arc<F>,
    policy_backend: Arc<dyn PolicyCheck + Send + Sync + 'static>,
    runner_backend: BackendRunner,
) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    println!("🚀 Policy: Daemon (If-Disconnected).");
    println!("🛡️ Daemon Policy: Checking network status via backend...");

    // The check logic is now 100% backend-agnostic
    match policy_backend.is_connected().await {
        Ok(true) => {
            println!("🛡️ Daemon Policy: Backend reports ALREADY CONNECTED. Provisioner will not start.");
        }
        Ok(false) => {
            println!("🛡️ Daemon Policy: Backend reports NOT connected. Starting provisioner...");
            crate::runner::run_provisioning_server(frontend, runner_backend).await?;
        }
        Err(e) => {
            println!("🛡️ Daemon Policy: Backend check failed ({}). Assuming NOT connected...", e);
            crate::runner::run_provisioning_server(frontend, runner_backend).await?;
        }
    }
    Ok(())
}