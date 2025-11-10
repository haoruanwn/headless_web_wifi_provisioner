use serde::{Deserialize, Serialize};

/// 表示扫描到的单个 Wi-Fi 网络
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Network {
    pub ssid: String,
    pub signal: u8,       // 信号强度，0到100
    pub security: String, // "WPA2", "WPA", "Open" 等
}

/// /api/connect 的请求体
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionRequest {
    pub ssid: String,
    pub password: String,
}
