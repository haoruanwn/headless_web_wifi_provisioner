use crate::traits::UiAssetProvider;
use std::sync::Arc;

/// Minimal factory: only create_frontend is kept for compatibility.
pub fn create_frontend() -> Arc<dyn UiAssetProvider> {
    const UI_THEME_COUNT: usize = cfg!(feature = "ui_echo_mate") as usize
        + cfg!(feature = "ui_radxa_x4") as usize;
    const _: () = assert!(UI_THEME_COUNT == 1, "Please select exactly ONE UI theme feature (e.g. ui_echo_mate or ui_radxa_x4). Please enable exactly one.");
    // Make the const 'used' in all cfg permutations to avoid dead_code warnings
    let _ = UI_THEME_COUNT;

    // Disk provider for local development (mock)
    #[cfg(feature = "backend_mock")]
    {
        println!("💿 Frontend: Disk Provider selected (for local development)");
        return Arc::new(crate::frontends::provider_disk::DiskFrontend::new());
    }

    // Embedded provider for real device builds — only compile this branch
    // when one of the ui_* features is selected (avoids unresolved module errors).
    #[cfg(all(not(feature = "backend_mock"), feature = "ui_echo_mate"))]
    {
        println!("📦 Frontend: Embed Provider (echo-mate) selected");
        return Arc::new(crate::frontends::provider_embed::EmbedFrontend::new());
    }

    #[cfg(all(not(feature = "backend_mock"), feature = "ui_radxa_x4"))]
    {
        println!("📦 Frontend: Embed Provider (radxa_x4) selected");
        return Arc::new(crate::frontends::provider_embed::EmbedFrontend::new());
    }

    // If we reach here, no frontend was configured at compile time.
    #[cfg(all(not(feature = "backend_mock"), not(feature = "ui_echo_mate"), not(feature = "ui_radxa_x4")))]
    compile_error!("No UI frontend selected: enable `backend_mock` or one of the ui_* features (ui_echo_mate, ui_radxa_x4).");
}