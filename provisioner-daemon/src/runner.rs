use provisioner_core::{web_server, traits::UiAssetProvider};
use std::sync::Arc;

// 运行配网服务器的封装逻辑（现在接受由 main 创建并注入的后端）

#[cfg(feature = "backend_wpa_cli_TDM")]
pub async fn run_provisioning_server<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::wpa_cli_TDM::WpaCliTdmBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    web_server::start_tdm_server(backend, frontend).await??;
    Ok(())
}

#[cfg(feature = "backend_networkmanager_TDM")]
pub async fn run_provisioning_server<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::networkmanager_TDM::NetworkManagerTdmBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    web_server::start_tdm_server(backend, frontend).await??;
    Ok(())
}

// WPA D-Bus backend support removed; keep implementations for remaining backends only.

#[cfg(feature = "backend_mock")]
pub async fn run_provisioning_server<F>(frontend: Arc<F>, backend: Arc<provisioner_core::backends::mock::MockBackend>) -> anyhow::Result<()>
where
    F: UiAssetProvider + 'static,
{
    web_server::start_concurrent_server(backend, frontend).await??;
    Ok(())
}

