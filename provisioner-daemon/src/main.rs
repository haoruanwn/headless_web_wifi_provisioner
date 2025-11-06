use provisioner_core::traits::{UiAssetProvider, ProvisioningTerminator, TdmBackend, ConcurrentBackend};
use std::sync::Arc;
use crate::runner::BackendRunner; // Import the Enum

mod runner;
mod policy;

// create_static_frontend() remains unchanged
fn create_static_frontend() -> Arc<impl UiAssetProvider + 'static> {
    const UI_THEME_COUNT: usize = cfg!(feature = "ui_echo_mate") as usize + cfg!(feature = "ui_radxa_x4") as usize;
    const _: () = assert!(UI_THEME_COUNT == 1, "Select exactly ONE UI theme.");
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

// 1. New function to create both trait objects
//    - policy_backend: For the policy layer (needs is_connected)
//    - runner_backend: For the execution layer (needs TDM/Concurrent specifics)
fn create_static_backend() -> anyhow::Result<(
    Arc<dyn ProvisioningTerminator + Send + Sync + 'static>,
    BackendRunner,
)> {
    
    #[cfg(feature = "backend_wpa_cli_TDM")]
    {
        println!("📡 Backend: WPA CLI TDM (Static Dispatch)");
        let backend = Arc::new(provisioner_core::backends::wpa_cli_TDM::WpaCliTdmBackend::new()?);
        // Return two Arcs: one for the policy (TdmBackend implements ProvisioningTerminator)
        // and the other for the runner (wrapped in the Enum).
        return Ok((backend.clone(), BackendRunner::Tdm(backend)));
    }

    #[cfg(feature = "backend_networkmanager_TDM")]
    {
        println!("📡 Backend: NetworkManager TDM (Static Dispatch)");
        let backend = Arc::new(
            provisioner_core::backends::networkmanager_TDM::NetworkManagerTdmBackend::new()?,
        );
        return Ok((backend.clone(), BackendRunner::Tdm(backend)));
    }

    #[cfg(feature = "backend_mock")]
    {
        println!("🔧 Backend: MockBackend (Static Dispatch)");
        let backend = Arc::new(provisioner_core::backends::mock::MockBackend::new());
        return Ok((backend.clone(), BackendRunner::Concurrent(backend)));
    }

    // Compile-time check
    #[cfg(not(any(
        feature = "backend_wpa_cli_TDM",
        feature = "backend_networkmanager_TDM",
        feature = "backend_mock"
    )))]
    compile_error!("Select exactly ONE backend feature.");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    println!("🚀 Starting provisioner-daemon...");

    let frontend = create_static_frontend();
    
    // 2. Create and destructure the two trait objects
    let (policy_backend, runner_backend) = create_static_backend()?;

    // 3. Inject the trait objects into policy::dispatch, no more cfg needed here
    policy::dispatch(frontend, policy_backend, runner_backend).await?;

    Ok(())
}
