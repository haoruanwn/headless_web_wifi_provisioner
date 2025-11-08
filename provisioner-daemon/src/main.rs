use provisioner_core::traits::{UiAssetProvider, PolicyCheck, TdmBackend, ConcurrentBackend};
use std::sync::Arc;
use crate::runner::BackendRunner; // Import the Enum

mod runner;
mod policy;

// create_static_frontend() remains unchanged
fn create_static_frontend() -> Arc<impl UiAssetProvider + 'static> {
    const UI_THEME_COUNT: usize = cfg!(feature = "ui_echo_mate") as usize + cfg!(feature = "ui_radxa_x4") as usize;
    const _: () = assert!(UI_THEME_COUNT == 1, "Select exactly ONE UI theme.");
    let _ = UI_THEME_COUNT;

    #[cfg(any(feature = "backend_mock_concurrent", feature = "backend_mock_TDM"))]
    {
        println!("💿 Frontend: Disk Provider selected (Static Dispatch)");
        Arc::new(provisioner_core::frontends::provider_disk::DiskFrontend::new())
    }
    #[cfg(not(any(feature = "backend_mock_concurrent", feature = "backend_mock_TDM")))]
    {
        println!("📦 Frontend: Embed Provider selected (Static Dispatch)");
        Arc::new(provisioner_core::frontends::provider_embed::EmbedFrontend::new())
    }
}

// 1. New function to create both trait objects
//    - policy_backend: For the policy layer (needs is_connected)
//    - runner_backend: For the execution layer (needs TDM/Concurrent specifics)
// 简化：仅返回 BackendRunner，由 policy 层再提取 PolicyCheck
fn create_static_backend() -> anyhow::Result<BackendRunner> {
    #[cfg(feature = "backend_wpa_cli_TDM")]
    {
        println!("📡 Backend: WPA CLI TDM (Static Dispatch)");
        let backend = Arc::new(provisioner_core::backends::wpa_cli_TDM::WpaCliTdmBackend::new()?);
        return Ok(BackendRunner::Tdm(backend));
    }

    #[cfg(feature = "backend_nmcli_TDM")]
    {
        println!("📡 Backend: NMCLI TDM (Static Dispatch)");
        let backend = Arc::new(provisioner_core::backends::nmcli_TDM::NmcliTdmBackend::new()?);
        return Ok(BackendRunner::Tdm(backend));
    }

    #[cfg(feature = "backend_nmdbus_TDM")]
    {
        println!("📡 Backend: NMDBUS TDM (Static Dispatch)");
        let backend = Arc::new(provisioner_core::backends::nmdbus_tdm::NmdbusTdmBackend::new()?);
        return Ok(BackendRunner::Tdm(backend));
    }

    #[cfg(feature = "backend_wpa_dbus_TDM")]
    {
        println!("📡 Backend: WPA DBUS TDM (Static Dispatch)");
        let backend = Arc::new(provisioner_core::backends::wpa_dbus_TDM::WpaDbusTdmBackend::new()?);
        return Ok(BackendRunner::Tdm(backend));
    }

    #[cfg(feature = "backend_mock_concurrent")]
    {
        println!("🔧 Backend: Mock Concurrent (Static Dispatch)");
        let backend = Arc::new(provisioner_core::backends::mock::MockConcurrentBackend::new());
        return Ok(BackendRunner::Concurrent(backend));
    }

    #[cfg(feature = "backend_mock_TDM")]
    {
        println!("🔧 Backend: Mock TDM (Static Dispatch)");
        let backend = Arc::new(provisioner_core::backends::mock::MockTdmBackend::new());
        return Ok(BackendRunner::Tdm(backend));
    }

    #[cfg(not(any(
        feature = "backend_wpa_cli_TDM",
        feature = "backend_nmcli_TDM",
        feature = "backend_nmdbus_TDM",
        feature = "backend_wpa_dbus_TDM",
        feature = "backend_mock_concurrent",
        feature = "backend_mock_TDM"
    )))]
    compile_error!("Select exactly ONE backend feature.");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    println!("🚀 Starting provisioner-daemon...");

    let frontend = create_static_frontend();
    
    // 2. Create and destructure the two trait objects
    let runner_backend = create_static_backend()?;
    // 由 policy::dispatch 自行根据 BackendRunner 抽取 PolicyCheck
    policy::dispatch(frontend, runner_backend).await?;

    Ok(())
}
