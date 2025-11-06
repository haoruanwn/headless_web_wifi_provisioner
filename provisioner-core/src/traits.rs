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

// -----------------------------------------------------------------------------
// 新增：按能力拆分的 trait（保持向后兼容）
// -----------------------------------------------------------------------------

/// 基础能力：终止/连接能力（所有后端都应提供）
#[async_trait]
pub trait ProvisioningTerminator: Send + Sync {
    /// 尝试连接（这是一个终止操作）
    async fn connect(&self, ssid: &str, password: &str) -> crate::Result<()>;

    /// 彻底退出配网模式（清理 AP）
    async fn exit_provisioning_mode(&self) -> crate::Result<()>;
}

/// 并发后端能力：支持实时扫描 + 启动 AP
#[async_trait]
pub trait ConcurrentBackend: ProvisioningTerminator {
    /// 进入配网模式（仅启动 AP）
    async fn enter_provisioning_mode(&self) -> crate::Result<()>;

    /// 执行一次实时的 Wi-Fi 扫描
    async fn scan(&self) -> crate::Result<Vec<Network>>;
}

/// TDM（时分复用）后端能力：启动时先扫描然后启动 AP，返回启动时的扫描列表
#[async_trait]
pub trait TdmBackend: ProvisioningTerminator {
    /// 进入配网模式并返回启动前的扫描列表
    async fn enter_provisioning_mode_with_scan(&self) -> crate::Result<Vec<Network>>;
}

// -----------------------------------------------------------------------------
// 兼容层：对于仍然实现了旧的 `ProvisioningBackend` 的后端，
// 自动为它们提供 `ProvisioningTerminator` / `ConcurrentBackend` 的实现，
// 这样可以在逐步迁移时保持向后兼容。
// -----------------------------------------------------------------------------

#[async_trait]
impl<T> ProvisioningTerminator for T
where
    T: ProvisioningBackend + Send + Sync,
{
    async fn connect(&self, ssid: &str, password: &str) -> crate::Result<()> {
        ProvisioningBackend::connect(self, ssid, password).await
    }

    async fn exit_provisioning_mode(&self) -> crate::Result<()> {
        ProvisioningBackend::exit_provisioning_mode(self).await
    }
}

#[async_trait]
impl<T> ConcurrentBackend for T
where
    T: ProvisioningBackend + Send + Sync,
{
    async fn enter_provisioning_mode(&self) -> crate::Result<()> {
        ProvisioningBackend::enter_provisioning_mode(self).await
    }

    async fn scan(&self) -> crate::Result<Vec<Network>> {
        ProvisioningBackend::scan(self).await
    }
}

