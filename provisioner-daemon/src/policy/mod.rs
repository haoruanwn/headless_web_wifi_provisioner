use provisioner_core::traits::UiAssetProvider;
use std::sync::Arc;

pub mod on_start;
pub mod daemon_if_disconnected;

/// 策略调度器：根据编译时选择的 policy feature 调用对应实现。
pub async fn dispatch(frontend: Arc<impl UiAssetProvider + 'static>) -> anyhow::Result<()> {
    const POLICY_COUNT: usize = cfg!(feature = "policy_on_start") as usize
        + cfg!(feature = "policy_daemon_if_disconnected") as usize;
    const _: () = assert!(POLICY_COUNT == 1, "Select exactly ONE policy feature (e.g., policy_on_start).");

    // reference to silence dead_code when some features are disabled
    let _ = POLICY_COUNT;

    #[cfg(feature = "policy_on_start")]
    {
        on_start::run(frontend).await?;
    }

    #[cfg(feature = "policy_daemon_if_disconnected")]
    {
        daemon_if_disconnected::run(frontend).await?;
    }

    Ok(())
}
