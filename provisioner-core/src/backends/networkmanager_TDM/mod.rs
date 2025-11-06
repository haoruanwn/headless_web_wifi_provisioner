// NetworkManager TDM backend (time-multiplexing)
// Minimal implementation using `nmcli` for scanning and `nmcli general` for state.
// This is intentionally conservative and best-effort; it mirrors the WpaCli TDM
// behaviour but uses NetworkManager where available.

use crate::traits::{Network, ProvisioningTerminator, TdmBackend};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;

const IFACE_NAME: &str = "wlan0";
const AP_IP_ADDR: &str = "192.168.4.1/24";

#[derive(Debug)]
pub struct NetworkManagerTdmBackend {
    // name of the hotspot connection created via nmcli (if any)
    hotspot_name: Arc<Mutex<Option<String>>>,
    last_scan: Arc<Mutex<Option<Vec<Network>>>>,
}

const PROV_SSID: &str = "ProvisionerAP";
const PROV_PSK: &str = "provisioner123";

impl NetworkManagerTdmBackend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            hotspot_name: Arc::new(Mutex::new(None)),
            last_scan: Arc::new(Mutex::new(None)),
        })
    }

    /// Start an AP using NetworkManager's nmcli hotspot command and remember the
    /// active connection name. Best-effort: if nmcli device wifi hotspot is
    /// unavailable, return an error.
    async fn start_ap(&self) -> Result<()> {
        // create a hotspot using NetworkManager
        let output = Command::new("nmcli")
            .arg("device")
            .arg("wifi")
            .arg("hotspot")
            .arg("ifname")
            .arg(IFACE_NAME)
            .arg("ssid")
            .arg(PROV_SSID)
            .arg("password")
            .arg(PROV_PSK)
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(Error::CommandFailed(format!(
                "Failed to start hotspot via nmcli: {}",
                err
            )));
        }

        // Find the active wifi connection name for this device
        let list = Command::new("nmcli")
            .arg("-t")
            .arg("-f")
            .arg("NAME,DEVICE,TYPE")
            .arg("connection")
            .arg("show")
            .arg("--active")
            .output()
            .await?;

        if list.status.success() {
            let txt = String::from_utf8_lossy(&list.stdout);
            for line in txt.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 3 {
                    let name = parts[0];
                    let device = parts[1];
                    let typ = parts[2];
                    if device == IFACE_NAME && typ.to_lowercase().contains("wifi") {
                        *self.hotspot_name.lock().await = Some(name.to_string());
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Stop the hotspot managed by NetworkManager (best-effort).
    async fn stop_ap(&self) -> Result<()> {
        if let Some(name) = self.hotspot_name.lock().await.take() {
            let _ = Command::new("nmcli")
                .arg("connection")
                .arg("down")
                .arg(&name)
                .output()
                .await;
            let _ = Command::new("nmcli")
                .arg("connection")
                .arg("delete")
                .arg(&name)
                .output()
                .await;
        } else {
            // fallback: try to bring down any active wifi connection on IFACE_NAME
            let list = Command::new("nmcli")
                .arg("-t")
                .arg("-f")
                .arg("NAME,DEVICE,TYPE")
                .arg("connection")
                .arg("show")
                .arg("--active")
                .output()
                .await?;
            if list.status.success() {
                let txt = String::from_utf8_lossy(&list.stdout);
                for line in txt.lines() {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() >= 3 {
                        let name = parts[0];
                        let device = parts[1];
                        let typ = parts[2];
                        if device == IFACE_NAME && typ.to_lowercase().contains("wifi") {
                            let _ = Command::new("nmcli")
                                .arg("connection")
                                .arg("down")
                                .arg(name)
                                .output()
                                .await;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn parse_nmcli_list(output: &str) -> Vec<Network> {
        // `nmcli -t -f SSID,SIGNAL,SECURITY device wifi list` yields colon-separated lines
        let mut networks = Vec::new();
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            // split into at most 3 fields
            let parts: Vec<&str> = line.split(':').collect();
            let ssid = parts.get(0).map(|s| s.to_string()).unwrap_or_default();
            if ssid.is_empty() || ssid == "\\x00" {
                continue;
            }
            let signal = parts
                .get(1)
                .and_then(|s| s.parse::<i16>().ok())
                .unwrap_or(0);
            let security = parts
                .get(2)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            let signal_percent = ((signal.clamp(-100, -50) + 100) * 2) as u8;
            networks.push(Network {
                ssid,
                signal: signal_percent,
                security,
            });
        }
        networks
    }

    async fn scan_internal(&self) -> Result<Vec<Network>> {
        // ask NetworkManager to rescan
        let _ = Command::new("nmcli")
            .arg("device")
            .arg("wifi")
            .arg("rescan")
            .output()
            .await;
        let output = Command::new("nmcli")
            .arg("-t")
            .arg("-f")
            .arg("SSID,SIGNAL,SECURITY")
            .arg("device")
            .arg("wifi")
            .arg("list")
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(Error::CommandFailed(format!("nmcli scan failed: {}", err)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parse_nmcli_list(&stdout))
    }

    // Check whether NetworkManager reports a connected state.
    pub async fn check_connected_nmcli() -> Result<bool> {
        match Command::new("nmcli")
            .arg("-t")
            .arg("-f")
            .arg("STATE")
            .arg("general")
            .output()
            .await
        {
            Ok(out) => {
                if !out.status.success() {
                    return Ok(false);
                }
                let s = String::from_utf8_lossy(&out.stdout).to_lowercase();
                Ok(s.contains("connected"))
            }
            Err(_) => Ok(false),
        }
    }
}

impl NetworkManagerTdmBackend {
    pub async fn enter_provisioning_mode_with_scan_impl(&self) -> Result<Vec<Network>> {
        // Ensure NetworkManager is running is out of scope; we rely on nmcli availability.
        let networks = self.scan_internal().await?;
        if networks.is_empty() {
            return Err(Error::CommandFailed(
                "Initial scan returned no networks".into(),
            ));
        }
        *self.last_scan.lock().await = Some(networks.clone());
        // start AP
        self.start_ap().await?;
        Ok(networks)
    }

    pub async fn connect_impl(&self, ssid: &str, password: &str) -> Result<()> {
        // Try to use nmcli to connect
        // For protected networks provide password, otherwise set open
        if password.is_empty() {
            let _ = Command::new("nmcli")
                .arg("device")
                .arg("wifi")
                .arg("connect")
                .arg(ssid)
                .output()
                .await;
        } else {
            let _ = Command::new("nmcli")
                .arg("device")
                .arg("wifi")
                .arg("connect")
                .arg(ssid)
                .arg("password")
                .arg(password)
                .output()
                .await;
        }
        // Best-effort: check connection state
        for _ in 0..15 {
            if let Ok(true) = Self::check_connected_nmcli().await {
                return Ok(());
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        // if failed, restore AP
        let _ = self.start_ap().await;
        Err(Error::CommandFailed(
            "Connection timed out or failed".into(),
        ))
    }

    async fn enter_provisioning_mode_impl(&self) -> Result<()> {
        // Similar to WpaCli: scan then start AP
        let networks = self.scan_internal().await?;
        if networks.is_empty() {
            return Err(Error::CommandFailed(
                "Initial scan returned no networks".into(),
            ));
        }
        *self.last_scan.lock().await = Some(networks);
        self.start_ap().await?;
        Ok(())
    }

    pub async fn scan_impl(&self) -> Result<Vec<Network>> {
        if let Some(vec) = &*self.last_scan.lock().await {
            return Ok(vec.clone());
        }
        let networks = self.scan_internal().await?;
        *self.last_scan.lock().await = Some(networks.clone());
        Ok(networks)
    }
}

#[async_trait]
impl ProvisioningTerminator for NetworkManagerTdmBackend {
    async fn is_connected(&self) -> Result<bool> {
        // Use `nmcli -t -f STATE general` which usually prints e.g. "connected" or "disconnected"
        match Command::new("nmcli")
            .arg("-t")
            .arg("-f")
            .arg("STATE")
            .arg("general")
            .output()
            .await
        {
            Ok(out) => {
                if !out.status.success() {
                    return Ok(false);
                }
                let s = String::from_utf8_lossy(&out.stdout).to_lowercase();
                Ok(s.contains("connected"))
            }
            Err(_) => Ok(false),
        }
    }

    async fn connect(&self, ssid: &str, password: &str) -> Result<()> {
        self.connect_impl(ssid, password).await
    }

    async fn exit_provisioning_mode(&self) -> Result<()> {
        self.stop_ap().await
    }
}

#[async_trait]
impl TdmBackend for NetworkManagerTdmBackend {
    async fn enter_provisioning_mode_with_scan(&self) -> Result<Vec<Network>> {
        self.enter_provisioning_mode_with_scan_impl().await
    }
}
