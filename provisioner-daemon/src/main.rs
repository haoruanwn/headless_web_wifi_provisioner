use provisioner_core::web_server;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 Starting provisioner-daemon...");

    // --- Compile-time validation to ensure exactly one backend is selected ---
    const BACKEND_COUNT: usize = cfg!(feature = "backend_mock") as usize +
        cfg!(feature = "backend_wpa_dbus") as usize +
        cfg!(feature = "backend_systemd") as usize;
    const _: () = assert!(BACKEND_COUNT == 1, "Please select exactly ONE backend feature.");

    // --- Compile-time validation to ensure exactly one UI theme is selected ---
    const UI_THEME_COUNT: usize = cfg!(feature = "ui_bootstrap") as usize +
        cfg!(feature = "ui_simple") as usize;
    const _: () = assert!(UI_THEME_COUNT == 1, "Please select exactly ONE UI theme feature: ui_bootstrap or ui_simple.");


    // --- Runtime instantiation based on the selected features ---

    #[cfg(feature = "backend_mock")]
    let backend: Arc<dyn provisioner_core::traits::ProvisioningBackend> = {
        println!("🔧 Backend: MockBackend selected");
        Arc::new(provisioner_core::backends::mock::MockBackend::new())
    };
    #[cfg(feature = "backend_wpa_dbus")]
    let backend: Arc<dyn provisioner_core::traits::ProvisioningBackend> = {
        println!("📡 Backend: WPA Supplicant (D-Bus) selected");
        Arc::new(provisioner_core::backends::wpa_supplicant_dbus::DbusBackend::new().await?)
    };
    #[cfg(feature = "backend_systemd")]
    let backend: Arc<dyn provisioner_core::traits::ProvisioningBackend> = {
        println!("🐧 Backend: Systemd Networkd selected");
        Arc::new(provisioner_core::backends::systemd_networkd::SystemdNetworkdBackend::new())
    };

    // Frontend provider is now chosen IMPLICITLY based on the backend selection.
    let frontend: Arc<dyn provisioner_core::traits::UiAssetProvider> = {
        // If mock backend is used, it implies local development. Use the Disk provider.
        #[cfg(feature = "backend_mock")]
        {
            println!("💿 Frontend: Disk Provider selected (for local development)");
            Arc::new(provisioner_core::frontends::provider_disk::DiskFrontend::new())
        }
        // If any real backend is used, it implies a release build. Use the Embed provider.
        #[cfg(not(feature = "backend_mock"))]
        {
            println!("📦 Frontend: Embed Provider selected (for deployment)");
            Arc::new(provisioner_core::frontends::provider_embed::EmbedFrontend::new())
        }
    };

    // --- Setup Provisioning Mode ---
    println!("Setting up provisioning mode...");
    backend.enter_provisioning_mode().await?;
    println!("Provisioning mode setup complete.");

    // --- Start the Web Server --- 
    let web_server_handle = web_server::start_web_server(
        backend,
        frontend,
    );

    // Run the web server
    web_server_handle.await??;

    Ok(())
}
