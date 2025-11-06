use provisioner_core::traits::{ProvisioningTerminator, UiAssetProvider};
use std::sync::Arc;
use crate::runner::BackendRunner; // Import BackendRunner

// Remove all cfg blocks!
pub async fn run<F>(
    frontend: Arc<F>,
    _policy_backend: Arc<dyn ProvisioningTerminator + Send + Sync + 'static>, // Receive but not use
    runner_backend: BackendRunner,
) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    println!("🚀 Policy: On-Start. Entering provisioning mode immediately.");
    crate::runner::run_provisioning_server(frontend, runner_backend).await?;
    Ok(())
}