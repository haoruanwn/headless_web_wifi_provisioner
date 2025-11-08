use async_trait::async_trait;
use serde::Serialize;
use std::borrow::Cow;

// 在这里定义共享的 Result 类型，和为所有后端和前端定义的 trait。

/// Represents a single Wi-Fi network found during a scan.
/// Wi-Fi 扫描时单个网络的具体信息。
#[derive(Debug, Clone, Serialize)]
pub struct Network {
    pub ssid: String,
    pub signal: u8, // 信号强度，0到100
    pub security: String, // 无线网络安全性 "WPA2", "WEP", "Open"
}

/// 前端资源提供者接口。
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
// 策略层最小化能力：只关心连接状态
// 用于行为策略，比如守护进程模式下的“如果未连接则启动配网”
#[async_trait]
pub trait PolicyCheck: Send + Sync {
    /// 检查设备当前是否已连接到网络
    async fn is_connected(&self) -> crate::Result<bool>;
}

/// 并发后端能力：支持实时扫描 + 启动 AP + 终止操作
/// 要求实现 PolicyCheck 接口
#[async_trait]
pub trait ConcurrentBackend: PolicyCheck {
    /// 进入配网模式（仅启动 AP）
    async fn enter_provisioning_mode(&self) -> crate::Result<()>;

    /// 执行一次实时的 Wi-Fi 扫描
    async fn scan(&self) -> crate::Result<Vec<Network>>;

    /// 尝试连接
    async fn connect(&self, ssid: &str, password: &str) -> crate::Result<()>;

    /// 彻底退出配网模式（清理 AP）
    async fn exit_provisioning_mode(&self) -> crate::Result<()>;
}

/// TDM（时分复用）后端能力：启动时先扫描然后启动 AP，并提供终止操作
/// 要求实现 PolicyCheck 接口
#[async_trait]
pub trait TdmBackend: PolicyCheck {
    /// 进入配网模式并返回启动前的扫描列表
    async fn enter_provisioning_mode_with_scan(&self) -> crate::Result<Vec<Network>>;

    /// 尝试连接（终止操作）
    async fn connect(&self, ssid: &str, password: &str) -> crate::Result<()>;

    /// 彻底退出配网模式（清理 AP）
    async fn exit_provisioning_mode(&self) -> crate::Result<()>;
}
