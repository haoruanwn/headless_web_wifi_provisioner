use crate::runner::BackendRunner;
use provisioner_core::traits::{PolicyCheck, UiAssetProvider};
use std::sync::Arc; // Import BackendRunner

// Remove all cfg blocks!
pub async fn run<F>(
    frontend: Arc<F>,
    _policy_backend: Arc<dyn PolicyCheck + Send + Sync + 'static>, // Receive but not use
    runner_backend: BackendRunner,
) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    println!("🚀 Policy: On-Start. Entering provisioning mode immediately.");
    crate::runner::run_provisioning_server(frontend, runner_backend).await?;
    Ok(())
}
