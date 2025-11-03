
use crate::traits::UiAssetProvider;
use crate::{Error, Result};
use async_trait::async_trait;
use rust_embed::RustEmbed;
use std::borrow::Cow;

// Conditionally compile the Asset struct based on the selected UI theme feature.
// This is the standard way to handle multiple embed sources with rust-embed.
#[cfg(feature = "ui_bootstrap")]
#[derive(RustEmbed)]
#[folder = "../ui/themes/bootstrap/"]
struct Asset;

#[cfg(feature = "ui_simple")]
#[derive(RustEmbed)]
#[folder = "../ui/themes/simple/"]
struct Asset;

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
