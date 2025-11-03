use async_trait::async_trait;
use serde::Serialize;
use std::borrow::Cow;

// 在这里定义共享的 Result 类型，和为所有后端和前端定义的 trait。

/// Represents a single Wi-Fi network found during a scan.
#[derive(Debug, Clone, Serialize)]
pub struct Network {
    pub ssid: String,
    pub signal: u8, // Signal strength as a percentage (0-100)
    pub security: String, // e.g., "WPA2", "WEP", "Open"
}

/// Defines the interface for a Wi-Fi provisioning backend.
/// This trait abstracts the underlying mechanism (D-Bus, mock, etc.)
/// for scanning and connecting to Wi-Fi networks.
#[async_trait]
pub trait ProvisioningBackend: Send + Sync {
    /// Prepares and enters the provisioning mode.
    /// This typically involves starting an Access Point (e.g., hostapd),
    /// configuring an IP address, and starting DHCP/DNS services.
    /// Wi-Fi切换为AP模式，开始配网
    async fn enter_provisioning_mode(&self) -> crate::Result<()>;

    /// Exits the provisioning mode and cleans up resources.
    /// This typically involves stopping the Access Point, cleaning up the IP address,
    /// and switching the interface to station (STA) mode.
    /// Wi-Fi退出AP模式，清理资源
    async fn exit_provisioning_mode(&self) -> crate::Result<()>;

    /// Scans for available Wi-Fi networks.
    ///
    /// # Returns
    /// A `Result` containing a vector of `Network` structs on success,
    /// or a `crate::error::Error` on failure.
    /// 扫描可用的 Wi-Fi 网络。
    async fn scan(&self) -> crate::Result<Vec<Network>>;

    /// Attempts to connect to a Wi-Fi network.
    ///
    /// # Arguments
    /// * `ssid` - The SSID of the network to connect to.
    /// * `password` - The password for the network.
    ///
    /// # Returns
    /// A `Result` indicating success or failure.
    /// 尝试连接到 Wi-Fi 网络。
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
    /// 获取单个 UI 资源。
    async fn get_asset(&self, path: &str) -> crate::Result<(Cow<'static, [u8]>, String)>;
}
