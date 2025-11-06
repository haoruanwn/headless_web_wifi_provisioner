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

// NOTE: Old `ProvisioningBackend` (dyn-based) removed.
// We now use capability-based traits below: `ProvisioningTerminator`,
// `ConcurrentBackend`, and `TdmBackend`.


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

// No compatibility blanket impls: we require explicit implementations
// of `ConcurrentBackend` or `TdmBackend` for selected backends.

