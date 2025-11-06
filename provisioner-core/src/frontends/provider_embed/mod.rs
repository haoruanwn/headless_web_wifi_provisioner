use crate::traits::UiAssetProvider;
use crate::{Error, Result};
use async_trait::async_trait;
use rust_embed::RustEmbed;
use std::borrow::Cow;

// --- Use $CARGO_MANIFEST_DIR to create an absolute path at compile time ---
#[cfg(feature = "ui_echo_mate")]
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../ui/themes/echo-mate/"]
struct Asset;

// --- Use $CARGO_MANIFEST_DIR to create an absolute path at compile time ---
#[cfg(feature = "ui_radxa_x4")]
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../ui/themes/radxa_x4/"]
struct AssetRadxa;

// Provide a small shim so the rest of the code can use `Asset` name.
#[cfg(feature = "ui_radxa_x4")]
use AssetRadxa as Asset;

/// A UI asset provider that serves files embedded into the binary.
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
        // The `Asset` struct used here is determined at compile time by the features above.
        let asset = Asset::get(path).ok_or_else(|| Error::AssetNotFound(path.to_string()))?;
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        Ok((asset.data, mime))
    }
}