use crate::traits::ApConfig;
use serde::Deserialize;
use std::net::SocketAddr;
use std::str::FromStr;

#[derive(Deserialize)]
struct ApConfigFile {
    ap_ssid: String,
    ap_psk: String,
    ap_gateway_cidr: String,
    ap_bind_addr: String,
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
        }
    }
}

pub fn ap_config_from_toml_str(s: &str) -> ApConfig {
    let parsed: ApConfigFile = toml::from_str(s).expect("Failed to parse AP config TOML");
    ApConfig::from(parsed)
}
