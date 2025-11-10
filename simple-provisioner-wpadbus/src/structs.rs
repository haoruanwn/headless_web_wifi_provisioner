use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// 表示扫描到的单个 Wi-Fi 网络
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Network {
    pub ssid: String,
    pub signal: u8,       // 信号强度，0到100
    pub security: String, // "WPA2", "WPA", "Open" 等
}

/// AP 配置
#[derive(Debug, Clone)]
pub struct ApConfig {
    /// AP 的网络名称 (SSID)
    pub ssid: String,
    /// AP 的密码 (PSK)
    pub psk: String,
    /// Web 服务器绑定的 Socket 地址
    pub bind_addr: SocketAddr,
    /// 网关和子网 (e.g., "192.168.4.1/24")
    pub gateway_cidr: String,
}

/// /api/connect 的请求体
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionRequest {
    pub ssid: String,
    pub password: String,
}
