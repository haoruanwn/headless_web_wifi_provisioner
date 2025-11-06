use crate::{Result};
use crate::traits::Network;

/// Parse `wpa_cli scan_results`-style output into Vec<Network>.
/// Returns crate::Result<Vec<Network>> to reuse existing error type.
pub fn parse_scan_results(output: &str) -> Result<Vec<Network>> {
    let mut networks = Vec::new();
    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 5 {
            let signal_level: i16 = parts[2].parse().unwrap_or(0);
            let flags = parts[3];
            let ssid = parts[4].to_string();

            if ssid.is_empty() || ssid == "\\x00" {
                continue;
            }

            let security = if flags.contains("WPA2") {
                "WPA2".to_string()
            } else if flags.contains("WPA") {
                "WPA".to_string()
            } else if flags.contains("WEP") {
                "WEP".to_string()
            } else {
                "Open".to_string()
            };

            let signal_percent = ((signal_level.clamp(-100, -50) + 100) * 2) as u8;

            networks.push(Network {
                ssid,
                signal: signal_percent,
                security,
            });
        }
    }
    Ok(networks)
}
