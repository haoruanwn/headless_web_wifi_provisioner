use provisioner_core::traits::{UiAssetProvider, ProvisioningTerminator};
use std::sync::Arc;
use crate::runner::BackendRunner; // Import BackendRunner

pub mod on_start;
pub mod daemon_if_disconnected;

// Remove all cfg blocks!
pub async fn dispatch<F>(
    frontend: Arc<F>,
    policy_backend: Arc<dyn ProvisioningTerminator + Send + Sync + 'static>,
    runner_backend: BackendRunner,
) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    // The policy selection logic remains unchanged
    const POLICY_COUNT: usize = cfg!(feature = "policy_on_start") as usize
        + cfg!(feature = "policy_daemon_if_disconnected") as usize;
    const _: () = assert!(POLICY_COUNT == 1, "Select exactly ONE policy...");
    let _ = POLICY_COUNT;

    #[cfg(feature = "policy_on_start")]
    {
        // Pass the trait object and enum
        on_start::run(frontend.clone(), policy_backend.clone(), runner_backend).await?;
    }

    #[cfg(feature = "policy_daemon_if_disconnected")]
    {
        daemon_if_disconnected::run(frontend, policy_backend, runner_backend).await?;
    }

    Ok(())
}