use provisioner_core::traits::{UiAssetProvider, ProvisioningTerminator};
use std::sync::Arc;

mod runner;
mod policy;

// 静态分发的前端工厂
fn create_static_frontend() -> Arc<impl UiAssetProvider + 'static> {
    // 编译时验证：确保只选择一个 UI 主题
    const UI_THEME_COUNT: usize = cfg!(feature = "ui_echo_mate") as usize + cfg!(feature = "ui_radxa_x4") as usize;
    const _: () = assert!(UI_THEME_COUNT == 1, "Select exactly ONE UI theme.");
    // reference to silence dead_code when a cfg branch returns early
    let _ = UI_THEME_COUNT;

    #[cfg(feature = "backend_mock")]
    {
        println!("💿 Frontend: Disk Provider selected (Static Dispatch)");
        Arc::new(provisioner_core::frontends::provider_disk::DiskFrontend::new())
    }
    #[cfg(not(feature = "backend_mock"))]
    {
        println!("📦 Frontend: Embed Provider selected (Static Dispatch)");
        Arc::new(provisioner_core::frontends::provider_embed::EmbedFrontend::new())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    println!("🚀 Starting provisioner-daemon...");

    let frontend = create_static_frontend();

    // --- Create backend early and inject into policy ---
    // 编译时验证：确保只选择一个后端（保留的后端：mock / wpa_cli_TDM / networkmanager_TDM）
    const BACKEND_COUNT: usize = cfg!(feature = "backend_mock") as usize
        + cfg!(feature = "backend_wpa_cli_TDM") as usize
        + cfg!(feature = "backend_networkmanager_TDM") as usize;
    const _: () = assert!(BACKEND_COUNT == 1, "Select exactly ONE backend.");
    let _ = BACKEND_COUNT;

    #[cfg(feature = "backend_wpa_cli_TDM")]
    {
        println!("📡 Backend: WPA CLI TDM (Static Dispatch)");
        let backend = Arc::new(provisioner_core::backends::wpa_cli_TDM::WpaCliTdmBackend::new()?);
        policy::dispatch(frontend, backend).await?;
    }

    #[cfg(feature = "backend_networkmanager_TDM")]
    {
        println!("📡 Backend: NetworkManager TDM (Static Dispatch)");
        let backend = Arc::new(
            provisioner_core::backends::networkmanager_TDM::NetworkManagerTdmBackend::new()?
        );
        policy::dispatch(frontend, backend).await?;
    }

    // Note: the WPA D-Bus backend was removed from the supported feature set.

    #[cfg(feature = "backend_mock")]
    {
        println!("🔧 Backend: MockBackend (Static Dispatch)");
        let backend = Arc::new(provisioner_core::backends::mock::MockBackend::new());
        policy::dispatch(frontend, backend).await?;
    }

    Ok(())
}