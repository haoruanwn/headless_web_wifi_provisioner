use provisioner_core::traits::UiAssetProvider;
use std::sync::Arc;

pub mod on_start;
pub mod daemon_if_disconnected;

/// 策略调度器：根据编译时选择的 policy feature 调用对应实现。

#[cfg(feature = "backend_wpa_cli_TDM")]
pub async fn dispatch<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::wpa_cli_TDM::WpaCliTdmBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    const POLICY_COUNT: usize = cfg!(feature = "policy_on_start") as usize
        + cfg!(feature = "policy_daemon_if_disconnected") as usize;
    const _: () = assert!(POLICY_COUNT == 1, "Select exactly ONE policy feature (e.g., policy_on_start).");
    let _ = POLICY_COUNT;

    #[cfg(feature = "policy_on_start")]
    {
        on_start::run(frontend.clone(), backend.clone()).await?;
    }

    #[cfg(feature = "policy_daemon_if_disconnected")]
    {
        daemon_if_disconnected::run(frontend, backend).await?;
    }

    Ok(())
}

#[cfg(feature = "backend_networkmanager_TDM")]
pub async fn dispatch<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::networkmanager_TDM::NetworkManagerTdmBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    const POLICY_COUNT: usize = cfg!(feature = "policy_on_start") as usize
        + cfg!(feature = "policy_daemon_if_disconnected") as usize;
    const _: () = assert!(POLICY_COUNT == 1, "Select exactly ONE policy feature (e.g., policy_on_start).");
    let _ = POLICY_COUNT;

    #[cfg(feature = "policy_on_start")]
    {
        on_start::run(frontend.clone(), backend.clone()).await?;
    }

    #[cfg(feature = "policy_daemon_if_disconnected")]
    {
        daemon_if_disconnected::run(frontend, backend).await?;
    }

    Ok(())
}

#[cfg(feature = "backend_wpa_dbus")]
pub async fn dispatch<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::wpa_supplicant_dbus::DbusBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    const POLICY_COUNT: usize = cfg!(feature = "policy_on_start") as usize
        + cfg!(feature = "policy_daemon_if_disconnected") as usize;
    const _: () = assert!(POLICY_COUNT == 1, "Select exactly ONE policy feature (e.g., policy_on_start).");
    let _ = POLICY_COUNT;

    #[cfg(feature = "policy_on_start")]
    {
        on_start::run(frontend.clone(), backend.clone()).await?;
    }

    #[cfg(feature = "policy_daemon_if_disconnected")]
    {
        daemon_if_disconnected::run(frontend, backend).await?;
    }

    Ok(())
}

#[cfg(feature = "backend_mock")]
pub async fn dispatch<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::mock::MockBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    const POLICY_COUNT: usize = cfg!(feature = "policy_on_start") as usize
        + cfg!(feature = "policy_daemon_if_disconnected") as usize;
    const _: () = assert!(POLICY_COUNT == 1, "Select exactly ONE policy feature (e.g., policy_on_start).");
    let _ = POLICY_COUNT;

    #[cfg(feature = "policy_on_start")]
    {
        on_start::run(frontend.clone(), backend.clone()).await?;
    }

    #[cfg(feature = "policy_daemon_if_disconnected")]
    {
        daemon_if_disconnected::run(frontend, backend).await?;
    }

    Ok(())
}
