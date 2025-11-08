use crate::config::ap_config_from_toml_str;
use crate::traits::{ApConfig, ConnectionRequest, Network, PolicyCheck, TdmBackend};
use crate::{Error, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;

// 通过调用nmcli命令行工具实现的TDM后端，适用于使用NetworkManager管理网络连接的Linux系统
const IFACE_NAME: &str = "wlan0";

static GLOBAL_AP_CONFIG: Lazy<ApConfig> = Lazy::new(|| {
    const CONFIG_TOML: &str = include_str!("../../../../configs/nmcli_tdm.toml");
    ap_config_from_toml_str(CONFIG_TOML)
});

#[derive(Debug)]
pub struct NmcliTdmBackend {
    ap_config: Arc<ApConfig>,
    hotspot_name: Arc<Mutex<Option<String>>>,
    last_scan: Arc<Mutex<Option<Vec<Network>>>>,
}

impl NmcliTdmBackend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            ap_config: Arc::new(GLOBAL_AP_CONFIG.clone()),
            hotspot_name: Arc::new(Mutex::new(None)),
            last_scan: Arc::new(Mutex::new(None)),
        })
    }

    /// 启动 AP（使用 `connection add` 以便指定 IP）
    async fn start_ap(&self) -> Result<()> {
        let ap_connection_name = &self.ap_config.ssid;

        let add_output = Command::new("nmcli")
            .arg("connection")
            .arg("add")
            .arg("type")
            .arg("wifi")
            .arg("ifname")
            .arg(IFACE_NAME)
            .arg("con-name")
            .arg(ap_connection_name)
            .arg("autoconnect")
            .arg("no")
            .arg("ssid")
            .arg(&self.ap_config.ssid)
            .arg("802-11-wireless.mode")
            .arg("ap")
            .arg("ipv4.method")
            .arg("shared")
            .arg("ipv4.addresses")
            .arg(&self.ap_config.gateway_cidr)
            .arg("wifi-sec.key-mgmt")
            .arg("wpa-psk")
            .arg("wifi-sec.psk")
            .arg(&self.ap_config.psk)
            .output()
            .await?;

        if !add_output.status.success() {
            let err = String::from_utf8_lossy(&add_output.stderr);
            if !err.contains("already exists") {
                return Err(Error::CommandFailed(format!(
                    "Failed to add hotspot connection: {}",
                    err
                )));
            }
        }

        let up_output = Command::new("nmcli")
            .arg("connection")
            .arg("up")
            .arg(ap_connection_name)
            .output()
            .await?;

        if !up_output.status.success() {
            let err = String::from_utf8_lossy(&up_output.stderr);
            return Err(Error::CommandFailed(format!(
                "Failed to bring up hotspot connection: {}",
                err
            )));
        }

        *self.hotspot_name.lock().await = Some(ap_connection_name.to_string());
        Ok(())
    }

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
        }
        Ok(())
    }

    fn parse_nmcli_list(output: &str) -> Vec<Network> {
        let mut networks = Vec::new();
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
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

    async fn check_connected_to_ssid(ssid: &str) -> Result<bool> {
        let output = Command::new("nmcli")
            .arg("-t")
            .arg("-f")
            .arg("NAME,DEVICE,STATE")
            .arg("connection")
            .arg("show")
            .arg("--active")
            .output()
            .await;
        match output {
            Ok(out) => {
                if !out.status.success() {
                    return Ok(false);
                }
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() >= 3 {
                        if parts[0] == ssid && parts[1] == IFACE_NAME && parts[2] == "activated" {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            Err(_) => Ok(false),
        }
    }

    pub async fn enter_provisioning_mode_with_scan_impl(&self) -> Result<Vec<Network>> {
        let networks = self.scan_internal().await?;
        if networks.is_empty() {
            return Err(Error::CommandFailed(
                "Initial scan returned no networks".into(),
            ));
        }
        *self.last_scan.lock().await = Some(networks.clone());
        self.start_ap().await?;
        Ok(networks)
    }

    pub async fn connect_impl(&self, ssid: &str, password: &str) -> Result<()> {
        self.stop_ap().await?;
        let _ = Command::new("nmcli")
            .arg("device")
            .arg("disconnect")
            .arg(IFACE_NAME)
            .status()
            .await;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let _ = Command::new("nmcli")
            .arg("device")
            .arg("wifi")
            .arg("rescan")
            .status()
            .await;
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        let connect_cmd = if password.is_empty() {
            Command::new("nmcli")
                .arg("device")
                .arg("wifi")
                .arg("connect")
                .arg(ssid)
                .spawn()
        } else {
            Command::new("nmcli")
                .arg("device")
                .arg("wifi")
                .arg("connect")
                .arg(ssid)
                .arg("password")
                .arg(password)
                .spawn()
        };
        if let Err(e) = connect_cmd {
            return Err(Error::Io(e));
        }

        for _ in 0..20 {
            if let Ok(true) = Self::check_connected_to_ssid(ssid).await {
                return Ok(());
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        let _ = self.start_ap().await;
        Err(Error::CommandFailed(
            format!("Connection to '{}' timed out (20s)", ssid).into(),
        ))
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
impl PolicyCheck for NmcliTdmBackend {
    async fn is_connected(&self) -> Result<bool> {
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

#[async_trait]
impl TdmBackend for NmcliTdmBackend {
    fn get_ap_config(&self) -> ApConfig {
        self.ap_config.as_ref().clone()
    }

    async fn enter_provisioning_mode_with_scan(&self) -> Result<Vec<Network>> {
        self.enter_provisioning_mode_with_scan_impl().await
    }

    async fn connect(&self, req: &ConnectionRequest) -> Result<()> {
        self.connect_impl(&req.ssid, &req.password).await
    }

    async fn exit_provisioning_mode(&self) -> Result<()> {
        self.stop_ap().await
    }
}
