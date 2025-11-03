
use provisioner_core::web_server::{self, AppState};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 Starting provisioner-daemon...");

    // --- Compile-time Dependency Injection via Feature Flags ---

    #[cfg(feature = "debug_build")]
    let state = {
        use provisioner_core::backends::mock_backend::MockBackend;
        use provisioner_core::frontends::disk_frontend::DiskFrontend;
        println!("🔧 Running in DEBUG mode (MockBackend, DiskFrontend)");
        AppState {
            backend: Arc::new(MockBackend::new()),
            frontend: Arc::new(DiskFrontend::new("ui/".into())),
        }
    };

    #[cfg(feature = "release_build")]
    let state = {
        use provisioner_core::backends::dbus_backend::DbusBackend;
        use provisioner_core::frontends::embed_frontend::EmbedFrontend;
        println!("📦 Running in RELEASE mode (DbusBackend, EmbedFrontend)");
        AppState {
            backend: Arc::new(DbusBackend::new()),
            frontend: Arc::new(EmbedFrontend::new()),
        }
    };

    // --- Start the Web Server --- 
    // The web server is generic over the traits, so it doesn't care
    // about the concrete implementations.
    let web_server_handle = web_server::start_web_server(
        state.backend,
        state.frontend,
    );

    // Run the web server
    web_server_handle.await??;

    Ok(())
}
