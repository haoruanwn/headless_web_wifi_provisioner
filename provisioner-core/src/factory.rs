use crate::traits::UiAssetProvider;
use std::sync::Arc;

/// Minimal factory: only create_frontend is kept for compatibility.
pub fn create_frontend() -> Arc<dyn UiAssetProvider> {
    const UI_THEME_COUNT: usize = cfg!(feature = "ui_echo_mate") as usize;
    const _: () = assert!(UI_THEME_COUNT == 1, "Please select exactly ONE UI theme feature: ui_echo_mate.");

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
}