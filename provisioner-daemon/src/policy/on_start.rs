use provisioner_core::traits::UiAssetProvider;
use std::sync::Arc;

/// On-Start 策略：程序启动时立即进入配网模式

#[cfg(feature = "backend_wpa_cli_TDM")]
pub async fn run<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::wpa_cli_TDM::WpaCliTdmBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    println!("🚀 Policy: On-Start. Entering provisioning mode immediately.");
    crate::runner::run_provisioning_server(frontend, backend).await?;
    Ok(())
}

#[cfg(feature = "backend_networkmanager_TDM")]
pub async fn run<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::networkmanager_TDM::NetworkManagerTdmBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    println!("🚀 Policy: On-Start. Entering provisioning mode immediately.");
    crate::runner::run_provisioning_server(frontend, backend).await?;
    Ok(())
}

// backend_wpa_dbus specialization removed
#[cfg(feature = "backend_mock")]
pub async fn run<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::mock::MockBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    println!("🚀 Policy: On-Start. Entering provisioning mode immediately.");
    crate::runner::run_provisioning_server(frontend, backend).await?;
    Ok(())
}
