use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use axum::response::IntoResponse;

#[derive(Serialize, Deserialize)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: i16,
}

#[derive(Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub success: bool,
    pub message: String,
}

/// "配网后端" 策略接口
#[async_trait]
pub trait ProvisioningBackend: Send + Sync + 'static {
    async fn scan(&self) -> Result<Vec<WifiNetwork>, String>;
    async fn connect(&self, ssid: &str, password: &str) -> Result<ConnectionStatus, String>;
}

/// "Web UI 前端" 策略接口
#[async_trait]
pub trait UiAssetProvider: Send + Sync + 'static {
    /// 根据路径获取一个 Web 资产
    async fn get_asset(&self, path: &str) -> Result<impl IntoResponse, impl IntoResponse>;
}