use crate::traits::{Network, ProvisioningBackend};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tokio::process::{Child, Command};

const IFACE_NAME: &str = "wlan0";
const AP_IP_ADDR: &str = "192.168.4.1/24";

/// A backend that uses `wpa_cli` and `dnsmasq` command-line tools.
#[derive(Debug)]
pub struct WpaCliDnsmasqBackend {
    hostapd: Arc<Mutex<Option<Child>>>,
    dnsmasq: Arc<Mutex<Option<Child>>>,
}

impl WpaCliDnsmasqBackend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            hostapd: Arc::new(Mutex::new(None)),
            dnsmasq: Arc::new(Mutex::new(None)),
        })
    }
}

#[async_trait]
impl ProvisioningBackend for WpaCliDnsmasqBackend {
    async fn enter_provisioning_mode(&self) -> Result<()> {
        println!("📡 [WpaCliDnsmasqBackend] Entering provisioning mode...");

        // 1. Set IP address for IFACE_NAME
        let output = Command::new("ip")
            .arg("addr")
            .arg("add")
            .arg(AP_IP_ADDR)
            .arg("dev")
            .arg(IFACE_NAME)
            .output()
            .await?;
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            if !error_msg.contains("File exists") {
                return Err(Error::CommandFailed(format!(
                    "Failed to set IP address: {}",
                    error_msg
                )));
            }
        }

        // 2. Start hostapd
        let hostapd_child = Command::new("hostapd")
            .arg("/etc/hostapd.conf")
            .arg("-B")
            .spawn()?;
        *self.hostapd.lock().unwrap() = Some(hostapd_child);

        // 3. Start dnsmasq
        let ap_ip_only = AP_IP_ADDR.split('/').next().unwrap_or("");
        let dnsmasq_child = Command::new("dnsmasq")
            .arg(format!("--interface={}", IFACE_NAME))
            .arg("--dhcp-range=192.168.4.100,192.168.4.200,12h")
            .arg(format!("--address=/#/{}", ap_ip_only))
            .arg("--no-resolv")
            .arg("--no-hosts")
            .spawn()?;
        *self.dnsmasq.lock().unwrap() = Some(dnsmasq_child);

        Ok(())
    }

    async fn exit_provisioning_mode(&self) -> Result<()> {
        println!("📡 [WpaCliDnsmasqBackend] Exiting provisioning mode...");

        let dnsmasq_child_to_kill = self.dnsmasq.lock().unwrap().take();
        if let Some(mut child) = dnsmasq_child_to_kill {
            child.kill().await?;
        }

        let hostapd_child_to_kill = self.hostapd.lock().unwrap().take();
        if let Some(mut child) = hostapd_child_to_kill {
            child.kill().await?;
        }

        let output = Command::new("ip")
            .arg("addr")
            .arg("del")
            .arg(AP_IP_ADDR)
            .arg("dev")
            .arg(IFACE_NAME)
            .output()
            .await?;
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            if !error_msg.contains("Cannot assign requested address") {
                return Err(Error::CommandFailed(format!(
                    "Failed to clean up IP address: {}",
                    error_msg
                )));
            }
        }

        println!("📡 [WpaCliDnsmasqBackend] Provisioning mode exited.");
        Ok(())
    }

    async fn scan(&self) -> Result<Vec<Network>> {
        println!("📡 [WpaCliDnsmasqBackend] Scanning for networks...");

        let output = Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("scan")
            .output()
            .await?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            if error_msg.contains("Failed to connect to wpa_supplicant") {
                return Err(Error::CommandFailed(
                    "wpa_supplicant is not running or not accessible".to_string(),
                ));
            }
            return Err(Error::CommandFailed(format!(
                "wpa_cli scan failed: {}",
                error_msg
            )));
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let output = Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("scan_results")
            .output()
            .await?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(Error::CommandFailed(format!(
                "wpa_cli scan_results failed: {}",
                error_msg
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_scan_results(&stdout)
    }

    async fn connect(&self, ssid: &str, password: &str) -> Result<()> {
        println!(
            "📡 [WpaCliDnsmasqBackend] Attempting to connect to SSID: '{}'...",
            ssid
        );

        let output = Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("add_network")
            .output()
            .await?;

        if !output.status.success() {
            return Err(Error::CommandFailed(
                "wpa_cli add_network failed".to_string(),
            ));
        }

        let network_id_str = String::from_utf8(output.stdout)
            .map_err(|e| Error::CommandFailed(format!("Failed to parse wpa_cli output: {}", e)))?;
        let network_id: u32 = network_id_str.trim().parse().map_err(|_| {
            Error::CommandFailed(format!(
                "Failed to parse network ID from wpa_cli: {}",
                network_id_str
            ))
        })?;

        println!("📡 [WpaCliDnsmasqBackend] Added network with ID: {}", network_id);

        let ssid_arg = format!("\"{}\"", ssid);
        Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("set_network")
            .arg(network_id.to_string())
            .arg("ssid")
            .arg(&ssid_arg)
            .status()
            .await?;

        if password.is_empty() {
            Command::new("wpa_cli")
                .arg("-i")
                .arg(IFACE_NAME)
                .arg("set_network")
                .arg(network_id.to_string())
                .arg("key_mgmt")
                .arg("NONE")
                .status()
                .await?;
        } else {
            let psk_arg = format!("\"{}\"", password);
            Command::new("wpa_cli")
                .arg("-i")
                .arg(IFACE_NAME)
                .arg("set_network")
                .arg(network_id.to_string())
                .arg("psk")
                .arg(&psk_arg)
                .status()
                .await?;
        }

        Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("enable_network")
            .arg(network_id.to_string())
            .status()
            .await?;

        Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("save_config")
            .status()
            .await?;

        Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("reconfigure")
            .status()
            .await?;

        println!(
            "📡 [WpaCliDnsmasqBackend] Connection process initiated for '{}'",
            ssid
        );
        Ok(())
    }
}

fn parse_scan_results(output: &str) -> Result<Vec<Network>> {
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

