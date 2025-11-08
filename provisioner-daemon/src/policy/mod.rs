use crate::runner::BackendRunner;
use provisioner_core::traits::{ConcurrentBackend, PolicyCheck, TdmBackend, UiAssetProvider};
use std::sync::Arc; // Import BackendRunner

pub mod daemon_if_disconnected;
pub mod on_start;

// Remove all cfg blocks!
pub async fn dispatch<F>(frontend: Arc<F>, runner_backend: BackendRunner) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    // The policy selection logic remains unchanged
    const POLICY_COUNT: usize = cfg!(feature = "policy_on_start") as usize
        + cfg!(feature = "policy_daemon_if_disconnected") as usize;
    const _: () = assert!(POLICY_COUNT == 1, "Select exactly ONE policy...");
    let _ = POLICY_COUNT;

    // 从 BackendRunner 中提取 PolicyCheck 引用（克隆 Arc）
    let policy_backend: Arc<dyn PolicyCheck + Send + Sync + 'static> = match &runner_backend {
        BackendRunner::Tdm(b) => b.clone() as Arc<dyn PolicyCheck + Send + Sync>,
        BackendRunner::Concurrent(b) => b.clone() as Arc<dyn PolicyCheck + Send + Sync>,
    };

    #[cfg(feature = "policy_on_start")]
    on_start::run(frontend.clone(), policy_backend.clone(), runner_backend).await?;

    #[cfg(feature = "policy_daemon_if_disconnected")]
    daemon_if_disconnected::run(frontend, policy_backend, runner_backend).await?;

    Ok(())
}
