use crate::traits::UiAssetProvider;
use crate::{Error, Result};
use async_trait::async_trait;
// 1. Comment out rust_embed
// use rust_embed::RustEmbed;
use std::borrow::Cow;

// 2. Import our new modules
use std::env;
use tokio::fs;

// -----------------------------------------------------------------
// 3. Comment out all RustEmbed related macros and structs
// -----------------------------------------------------------------
/*
#[cfg(feature = "ui_echo_mate") ]
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../ui/themes/echo-mate/"]
struct AssetEcho;

#[cfg(feature = "ui_echo_mate")]
use AssetEcho as Asset;


#[cfg(feature = "ui_radxa_x4")]
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../ui/themes/radxa_x4/"]
struct AssetRadxa;

// Provide a small shim so the rest of the code can use `Asset` name.
#[cfg(feature = "ui_radxa_x4")]
use AssetRadxa as Asset;
*/
// -----------------------------------------------------------------

/// A UI asset provider that serves files embedded into the binary.
/// (Note: For testing, we temporarily replace its logic with loading from disk)
#[derive(Debug, Default)]
pub struct EmbedFrontend;

impl EmbedFrontend {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl UiAssetProvider for EmbedFrontend {
    async fn get_asset(&self, path: &str) -> Result<(Cow<'static, [u8]>, String)> {
        // -----------------------------------------------------------------
        // 4. Insert the "executable relative path" disk loading logic
        // -----------------------------------------------------------------

        // A. Select theme path (this logic remains)
        #[cfg(feature = "ui_echo_mate")]
        let theme_path = "ui/themes/echo-mate";

        #[cfg(feature = "ui_radxa_x4")]
        let theme_path = "ui/themes/radxa_x4";

        // B. Get the directory where the executable is located (e.g., /target/release)
        let exe_path = env::current_exe().map_err(Error::Io)?;
        let exe_dir = exe_path.parent().ok_or_else(|| {
            Error::AssetNotFound("Failed to get executable's parent directory".to_string())
        })?;

        // C. Construct the absolute path of the asset
        //    (e.g., /target/release/ui/themes/radxa_x4/index.html)
        let asset_path = exe_dir.join(theme_path).join(path);

        // D. Read from disk
        let content = fs::read(&asset_path).await.map_err(|e| {
            Error::AssetNotFound(format!(
                "Asset not found. Looked for: {:?}. Error: {}",
                asset_path, e
            ))
        })?;

        // -----------------------------------------------------------------

        /*
        // 5. Comment out the original Asset::get logic
        let asset = Asset::get(path).ok_or_else(|| Error::AssetNotFound(path.to_string()))?;
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        Ok((asset.data, mime))
        */

        // 6. Return the content we read from disk
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        Ok((Cow::Owned(content), mime))
    }
}
