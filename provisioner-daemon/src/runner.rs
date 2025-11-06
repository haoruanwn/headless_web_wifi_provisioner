use provisioner_core::{web_server, traits::UiAssetProvider};
use std::sync::Arc;

// 运行配网服务器的封装逻辑
pub async fn run_provisioning_server(frontend: Arc<impl UiAssetProvider + 'static>) -> anyhow::Result<()> {
    // 编译时验证：确保只选择一个后端
    const BACKEND_COUNT: usize = cfg!(feature = "backend_mock") as usize
        + cfg!(feature = "backend_wpa_dbus") as usize
        + cfg!(feature = "backend_wpa_cli") as usize
        + cfg!(feature = "backend_wpa_cli_exclusive") as usize
        + cfg!(feature = "backend_wpa_cli_TDM") as usize
        + cfg!(feature = "backend_systemd") as usize;
    const _: () = assert!(BACKEND_COUNT == 1, "Select exactly ONE backend.");
    // reference to silence dead_code when cfg branches return early
    let _ = BACKEND_COUNT;

    // --- Branch: TDM backend ---
    #[cfg(feature = "backend_wpa_cli_TDM")]
    {
        println!("📡 Backend: WPA CLI TDM (Static Dispatch)");
        let backend = std::sync::Arc::new(provisioner_core::backends::wpa_cli_TDM::WpaCliTdmBackend::new()?);
        web_server::start_tdm_server(backend, frontend).await??;
    }

    // --- Branch: D-Bus (concurrent) ---
    #[cfg(feature = "backend_wpa_dbus")]
    {
        println!("📡 Backend: WPA Supplicant D-Bus (Static Dispatch)");
        let backend = std::sync::Arc::new(
            provisioner_core::backends::wpa_supplicant_dbus::DbusBackend::new().await?,
        );
        web_server::start_concurrent_server(backend, frontend).await??;
    }

    // --- Branch: Mock (concurrent) ---
    #[cfg(feature = "backend_mock")]
    {
        println!("🔧 Backend: MockBackend (Static Dispatch)");
        let backend = std::sync::Arc::new(provisioner_core::backends::mock::MockBackend::new());
        web_server::start_concurrent_server(backend, frontend).await??;
    }

    Ok(())
}
