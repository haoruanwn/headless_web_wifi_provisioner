
use crate::traits::UiAssetProvider;
use crate::{Error, Result};
use async_trait::async_trait;
use std::borrow::Cow;
use std::path::PathBuf;
use tokio::fs;

/// A UI asset provider that reads files directly from disk.
pub struct DiskFrontend;

impl DiskFrontend {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl UiAssetProvider for DiskFrontend {
    async fn get_asset(&self, path: &str) -> Result<(Cow<'static, [u8]>, String)> {
        // Select theme path at compile time based on features
        #[cfg(feature = "ui_bootstrap")]
        let theme_path = "ui/themes/bootstrap";
        #[cfg(feature = "ui_simple")]
        let theme_path = "ui/themes/simple";

        // Sanitize the path to prevent directory traversal attacks
        let asset_path = PathBuf::from(theme_path).join(path);

        // Read the file from disk
        let content = fs::read(asset_path)
            .await
            .map_err(|_| Error::AssetNotFound(path.to_string()))?;

        // Guess the MIME type based on the file extension
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        Ok((Cow::Owned(content), mime))
    }
}
