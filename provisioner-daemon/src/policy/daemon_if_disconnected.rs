use provisioner_core::traits::{ProvisioningTerminator, UiAssetProvider};
use std::sync::Arc;

/// 守护进程策略：仅当指定接口未连接时才进入配网模式

#[cfg(feature = "backend_wpa_cli_TDM")]
#[allow(dead_code)]
pub async fn run<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::wpa_cli_TDM::WpaCliTdmBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    println!("🚀 Policy: Daemon (If-Disconnected).");
    println!("🛡️ Daemon Policy: Checking network status via backend...");

    match backend.is_connected().await {
        Ok(true) => println!("🛡️ Daemon Policy: Backend reports ALREADY CONNECTED. Provisioner will not start."),
        Ok(false) => {
            println!("🛡️ Daemon Policy: Backend reports NOT connected. Starting provisioner...");
            crate::runner::run_provisioning_server(frontend, backend).await?;
        }
        Err(e) => {
            println!("🛡️ Daemon Policy: Backend check failed ({}). Assuming NOT connected and starting provisioner...", e);
            crate::runner::run_provisioning_server(frontend, backend).await?;
        }
    }

    Ok(())
}

#[cfg(feature = "backend_networkmanager_TDM")]
#[allow(dead_code)]
pub async fn run<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::networkmanager_TDM::NetworkManagerTdmBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    println!("🚀 Policy: Daemon (If-Disconnected).");
    println!("🛡️ Daemon Policy: Checking network status via backend...");

    match backend.is_connected().await {
        Ok(true) => println!("🛡️ Daemon Policy: Backend reports ALREADY CONNECTED. Provisioner will not start."),
        Ok(false) => {
            println!("🛡️ Daemon Policy: Backend reports NOT connected. Starting provisioner...");
            crate::runner::run_provisioning_server(frontend, backend).await?;
        }
        Err(e) => {
            println!("🛡️ Daemon Policy: Backend check failed ({}). Assuming NOT connected and starting provisioner...", e);
            crate::runner::run_provisioning_server(frontend, backend).await?;
        }
    }

    Ok(())
}

// backend_wpa_dbus specialization removed
#[cfg(feature = "backend_mock")]
#[allow(dead_code)]
pub async fn run<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::mock::MockBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    println!("🚀 Policy: Daemon (If-Disconnected).");
    println!("🛡️ Daemon Policy: Checking network status via backend...");

    match backend.is_connected().await {
        Ok(true) => println!("🛡️ Daemon Policy: Backend reports ALREADY CONNECTED. Provisioner will not start."),
        Ok(false) => {
            println!("🛡️ Daemon Policy: Backend reports NOT connected. Starting provisioner...");
            crate::runner::run_provisioning_server(frontend, backend).await?;
        }
        Err(e) => {
            println!("🛡️ Daemon Policy: Backend check failed ({}). Assuming NOT connected and starting provisioner...", e);
            crate::runner::run_provisioning_server(frontend, backend).await?;
        }
    }

    Ok(())
}
