
use async_trait::async_trait;
use serde::Serialize;
use std::borrow::Cow;

/// Represents a single Wi-Fi network found during a scan.
#[derive(Debug, Clone, Serialize)]
pub struct Network {
    pub ssid: String,
    // Note: You might add more fields here later, like signal strength, security type, etc.
}

/// Defines the interface for a Wi-Fi provisioning backend.
/// This trait abstracts the underlying mechanism (D-Bus, mock, etc.)
/// for scanning and connecting to Wi-Fi networks.
#[async_trait]
pub trait ProvisioningBackend: Send + Sync {
    /// Scans for available Wi-Fi networks.
    ///
    /// # Returns
    /// A `Result` containing a vector of `Network` structs on success,
    /// or a `crate::error::Error` on failure.
    async fn scan(&self) -> crate::Result<Vec<Network>>;

    /// Attempts to connect to a Wi-Fi network.
    ///
    /// # Arguments
    /// * `ssid` - The SSID of the network to connect to.
    /// * `password` - The password for the network.
    ///
    /// # Returns
    /// A `Result` indicating success or failure.
    async fn connect(&self, ssid: &str, password: &str) -> crate::Result<()>;
}


/// Defines the interface for providing UI assets.
/// This trait abstracts the source of the UI files, allowing them
/// to be loaded from disk (for debugging) or from an embedded resource
/// (for release).
#[async_trait]
pub trait UiAssetProvider: Send + Sync {
    /// Retrieves a single UI asset.
    ///
    /// # Arguments
    /// * `path` - The path to the asset (e.g., "index.html", "style.css").
    ///
    /// # Returns
    /// A `Result` containing a tuple of (`Cow<'static, [u8]>`, `String`)
    /// representing the asset's content and its MIME type, or an `Error` if not found.
    async fn get_asset(&self, path: &str) -> crate::Result<(Cow<'static, [u8]>, String)>;
}
