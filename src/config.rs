use serde::Deserialize;
use std::net::SocketAddr;
use std::str::FromStr;

/// AP 运行时配置（包含所有网络接口、路径、DHCP 等配置）
#[derive(Debug, Clone)]
pub struct ApConfig {
    // === AP 基本配置 ===
    pub ssid: String,
    pub psk: String,
    pub bind_addr: SocketAddr,
    pub gateway_cidr: String,

    // === 网络接口配置 ===
    pub interface_name: String,

    // === DHCP 配置 ===
    pub dhcp_range: String,

    // === 自包含配置文件路径 ===
    pub hostapd_conf_path: String,
    pub wpa_conf_path: String,

    // === wpa_supplicant 控制接口配置 ===
    pub wpa_ctrl_interface: String,
    pub wpa_group: String,
    pub wpa_update_config: bool,

    // === hostapd 无线配置 ===
    pub hostapd_hw_mode: String,
    pub hostapd_channel: u8,
    pub hostapd_wpa: u8,
    pub hostapd_wpa_key_mgmt: String,
    pub hostapd_wpa_pairwise: String,
    pub hostapd_rsn_pairwise: String,
}

#[derive(Deserialize)]
struct ApConfigFile {
    ap_ssid: String,
    ap_psk: String,
    ap_gateway_cidr: String,
    ap_bind_addr: String,

    interface_name: String,
    dhcp_range: String,
    hostapd_conf_path: String,
    wpa_conf_path: String,
    
    wpa_ctrl_interface: String,
    wpa_group: String,
    wpa_update_config: bool,
    
    hostapd_hw_mode: String,
    hostapd_channel: u8,
    hostapd_wpa: u8,
    hostapd_wpa_key_mgmt: String,
    hostapd_wpa_pairwise: String,
    hostapd_rsn_pairwise: String,
}

impl From<ApConfigFile> for ApConfig {
    fn from(t: ApConfigFile) -> Self {
        let bind_addr =
            SocketAddr::from_str(&t.ap_bind_addr).expect("Invalid ap_bind_addr in TOML");
        ApConfig {
            ssid: t.ap_ssid,
            psk: t.ap_psk,
            bind_addr,
            gateway_cidr: t.ap_gateway_cidr,

            interface_name: t.interface_name,
            dhcp_range: t.dhcp_range,
            hostapd_conf_path: t.hostapd_conf_path,
            wpa_conf_path: t.wpa_conf_path,
            
            wpa_ctrl_interface: t.wpa_ctrl_interface,
            wpa_group: t.wpa_group,
            wpa_update_config: t.wpa_update_config,
            
            hostapd_hw_mode: t.hostapd_hw_mode,
            hostapd_channel: t.hostapd_channel,
            hostapd_wpa: t.hostapd_wpa,
            hostapd_wpa_key_mgmt: t.hostapd_wpa_key_mgmt,
            hostapd_wpa_pairwise: t.hostapd_wpa_pairwise,
            hostapd_rsn_pairwise: t.hostapd_rsn_pairwise,
        }
    }
}

pub fn ap_config_from_toml_str(s: &str) -> ApConfig {
    let parsed: ApConfigFile = toml::from_str(s).expect("Failed to parse AP config TOML");
    ApConfig::from(parsed)
}
