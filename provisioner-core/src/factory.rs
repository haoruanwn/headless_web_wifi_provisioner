use crate::traits::{ProvisioningBackend, UiAssetProvider};
use std::sync::Arc;
use anyhow::Result;

/// 根据编译时特性，创建并返回一个 ProvisioningBackend 实例。
///
/// 所有的 `#[cfg]` 逻辑都被封装在这里。
pub async fn create_backend() -> Result<Arc<dyn ProvisioningBackend>> {
    // --- 编译时验证 (从 main.rs 移过来) ---
    const BACKEND_COUNT: usize = cfg!(feature = "backend_mock") as usize
        + cfg!(feature = "backend_wpa_dbus") as usize
        + cfg!(feature = "backend_wpa_cli") as usize
        + cfg!(feature = "backend_wpa_cli_exclusive") as usize
        + cfg!(feature = "backend_wpa_cli_TDM") as usize
        + cfg!(feature = "backend_systemd") as usize;
    const _: () = assert!(
        BACKEND_COUNT == 1,
        "Please select exactly ONE backend feature."
    );

    // --- 实例化逻辑 (从 main.rs 移过来) ---

    #[cfg(feature = "backend_mock")]
    let backend: Arc<dyn ProvisioningBackend> = {
        println!("🔧 Backend: MockBackend selected");
        Arc::new(crate::backends::mock::MockBackend::new())
    };

    #[cfg(feature = "backend_wpa_dbus")]
    let backend: Arc<dyn ProvisioningBackend> = {
        println!("📡 Backend: WPA Supplicant (D-Bus) selected");
        Arc::new(crate::backends::wpa_supplicant_dbus::DbusBackend::new().await?)
    };

    #[cfg(feature = "backend_systemd")]
    let backend: Arc<dyn ProvisioningBackend> = {
        println!("🐧 Backend: Systemd Networkd selected");
        Arc::new(crate::backends::systemd_networkd::SystemdNetworkdBackend::new())
    };

    #[cfg(feature = "backend_wpa_cli")]
    let backend: Arc<dyn ProvisioningBackend> = {
        println!("CLI Backend: WPA CLI + Dnsmasq selected");
        Arc::new(crate::backends::wpa_cli_dnsmasq::WpaCliDnsmasqBackend::new()?)
    };

    #[cfg(feature = "backend_wpa_cli_exclusive")]
    let backend: Arc<dyn ProvisioningBackend> = {
        println!("CLI Backend: WPA CLI Exclusive selected");
        Arc::new(crate::backends::wpa_cli_exclusive::WpaCliExclusiveBackend::new()?)
    };

    #[cfg(feature = "backend_wpa_cli_TDM")]
    let backend: Arc<dyn ProvisioningBackend> = {
        println!("CLI Backend: WPA CLI TDM selected");
        Arc::new(crate::backends::wpa_cli_TDM::WpaCliTdmBackend::new()?)
    };

    Ok(backend)
}

/// 根据编译时特性，创建并返回一个 UiAssetProvider 实例。
pub fn create_frontend() -> Arc<dyn UiAssetProvider> {
    // --- 编译时验证 (从 main.rs 移过来) ---
    const UI_THEME_COUNT: usize = cfg!(feature = "ui_echo_mate") as usize;
    const _: () = assert!(
        UI_THEME_COUNT == 1,
        "Please select exactly ONE UI theme feature: ui_echo_mate."
    );

    // --- 实例化逻辑 (从 main.rs 移过来) ---
    let frontend: Arc<dyn UiAssetProvider> = {
        #[cfg(feature = "backend_mock")]
        {
            println!("💿 Frontend: Disk Provider selected (for local development)");
            Arc::new(crate::frontends::provider_disk::DiskFrontend::new())
        }
        #[cfg(not(feature = "backend_mock"))]
        {
            println!("📦 Frontend: Embed Provider selected (for deployment)");
            Arc::new(crate::frontends::provider_embed::EmbedFrontend::new())
        }
    };

    frontend
}