
use crate::traits::UiAssetProvider;
use crate::{Error, Result};
use async_trait::async_trait;
use std::borrow::Cow;
use std::path::PathBuf;
use tokio::fs;

/// A UI asset provider that reads files directly from disk.
/// Ideal for development, as it allows for live reloading of UI assets.
pub struct DiskFrontend {
    pub ui_dir: PathBuf,
}

impl DiskFrontend {
    /// Creates a new `DiskFrontend`.
    ///
    /// # Arguments
    /// * `ui_dir` - The path to the directory containing the UI files (e.g., "ui/").
    pub fn new(ui_dir: PathBuf) -> Self {
        Self { ui_dir }
    }
}

#[async_trait]
impl UiAssetProvider for DiskFrontend {
    async fn get_asset(&self, path: &str) -> Result<(Cow<'static, [u8]>, String)> {
        // Sanitize the path to prevent directory traversal attacks
        let asset_path = self.ui_dir.join(path);

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
