use crate::config::ap_config_from_toml_str;
use crate::traits::{ApConfig, ConnectionRequest, Network, PolicyCheck, TdmBackend};
use crate::{Error, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::fs;
use zbus::{Connection, Proxy};

// MVP DBus backend for wpa_supplicant + external hostapd/dnsmasq for AP mode.
// Station operations (scan/connect) will use DBus where feasible; fallback to wpa_cli textual parsing for now.

static GLOBAL_AP_CONFIG: Lazy<ApConfig> = Lazy::new(|| {
    const CONFIG_TOML: &str = include_str!("../../../../configs/wpa_dbus_tdm.toml");
    ap_config_from_toml_str(CONFIG_TOML)
});

const IFACE_NAME: &str = "wlan0";
const WPA_SUPPLICANT_SERVICE: &str = "fi.w1.wpa_supplicant1";
const WPA_SUPPLICANT_PATH: &str = "/fi/w1/wpa_supplicant1"; // root manager path
const WPA_SUPPLICANT_INTERFACE: &str = "fi.w1.wpa_supplicant1";

#[derive(Debug)]
pub struct WpaDbusTdmBackend {
    ap_config: Arc<ApConfig>,
    hostapd: Arc<Mutex<Option<tokio::process::Child>>>,
    dnsmasq: Arc<Mutex<Option<tokio::process::Child>>>,
    last_scan: Arc<Mutex<Option<Vec<Network>>>>,
    conn: Arc<Mutex<Option<Connection>>>,
}

impl WpaDbusTdmBackend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            ap_config: Arc::new(GLOBAL_AP_CONFIG.clone()),
            hostapd: Arc::new(Mutex::new(None)),
            dnsmasq: Arc::new(Mutex::new(None)),
            last_scan: Arc::new(Mutex::new(None)),
            conn: Arc::new(Mutex::new(None)),
        })
    }

    async fn ensure_conn(&self) -> Result<Connection> {
        if let Some(c) = self.conn.lock().await.clone() { return Ok(c); }
        let c = Connection::system().await.map_err(|e| Error::CommandFailed(format!("DBus connect failed: {}", e)))?;
        *self.conn.lock().await = Some(c.clone());
        Ok(c)
    }

    async fn root_proxy(&self) -> Result<Proxy<'_>> {
        let conn = self.ensure_conn().await?;
        Proxy::builder(&conn)
            .destination(WPA_SUPPLICANT_SERVICE)
            .map_err(|e| Error::CommandFailed(format!("dest error: {}", e)))?
            .path(WPA_SUPPLICANT_PATH)
            .map_err(|e| Error::CommandFailed(format!("path error: {}", e)))?
            .interface(WPA_SUPPLICANT_INTERFACE)
            .map_err(|e| Error::CommandFailed(format!("iface error: {}", e)))?
            .build()
            .await
            .map_err(|e| Error::CommandFailed(format!("proxy build error: {}", e)))
    }

    async fn scan_internal(&self) -> Result<Vec<Network>> {
        // Fallback to wpa_cli textual scan for first MVP (DBus expects per-interface object resolution)
        let output = Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("scan").output().await?;
        if !output.status.success() {
            return Err(Error::CommandFailed("wpa_cli scan failed".into()));
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        let output = Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("scan_results").output().await?;
        if !output.status.success() {
            return Err(Error::CommandFailed("wpa_cli scan_results failed".into()));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut networks = Vec::new();
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                let signal_level: i16 = parts[2].parse().unwrap_or(0);
                let flags = parts[3];
                let ssid = parts[4].to_string();
                if ssid.is_empty() || ssid == "\x00" { continue; }
                let security = if flags.contains("WPA2") { "WPA2" } else if flags.contains("WPA") { "WPA" } else if flags.contains("WEP") { "WEP" } else { "Open" }.to_string();
                let signal_percent = ((signal_level.clamp(-100, -50) + 100) * 2) as u8;
                networks.push(Network { ssid, signal: signal_percent, security });
            }
        }
        Ok(networks)
    }

    async fn enter_with_scan_impl(&self) -> Result<Vec<Network>> {
        let networks = self.scan_internal().await?;
        if networks.is_empty() { return Err(Error::CommandFailed("Initial scan returned no networks".into())); }
        *self.last_scan.lock().await = Some(networks.clone());
        self.start_ap().await?;
        Ok(networks)
    }

    async fn start_ap(&self) -> Result<()> {
        let _ = Command::new("killall").arg("-9").arg("hostapd").arg("dnsmasq").arg("wpa_supplicant").status().await;
        let output = Command::new("ip").arg("addr").arg("add").arg(&self.ap_config.gateway_cidr).arg("dev").arg(IFACE_NAME).output().await?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            if !err.contains("File exists") { return Err(Error::CommandFailed(format!("Failed to set IP: {}", err))); }
        }
        let hostapd_conf = format!("interface={}\nssid={}\nwpa=2\nwpa_passphrase={}\nhw_mode=g\nchannel=6\nwpa_key_mgmt=WPA-PSK\nwpa_pairwise=CCMP\nrsn_pairwise=CCMP\n", IFACE_NAME, self.ap_config.ssid, self.ap_config.psk);
        let conf_path = "/tmp/provisioner_hostapd.conf";
        fs::write(conf_path, hostapd_conf.as_bytes()).await?;
        let child = Command::new("hostapd").arg(conf_path).arg("-B").spawn()?;
        *self.hostapd.lock().await = Some(child);
        let ap_ip_only = self.ap_config.gateway_cidr.split('/').next().unwrap_or("");
        let dnsmasq_child = Command::new("dnsmasq")
            .arg(format!("--interface={}", IFACE_NAME))
            .arg("--dhcp-range=192.168.4.100,192.168.4.200,12h")
            .arg(format!("--address=/#/{}", ap_ip_only))
            .arg("--no-resolv")
            .arg("--no-hosts")
            .arg("--no-daemon")
            .spawn()?;
        *self.dnsmasq.lock().await = Some(dnsmasq_child);
        Ok(())
    }

    async fn stop_ap(&self) -> Result<()> {
        if let Some(mut child) = self.dnsmasq.lock().await.take() { let _ = child.kill().await; }
        if let Some(mut child) = self.hostapd.lock().await.take() { let _ = child.kill().await; }
        let output = Command::new("ip").arg("addr").arg("del").arg(&self.ap_config.gateway_cidr).arg("dev").arg(IFACE_NAME).output().await?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            if !err.contains("Cannot assign requested address") { return Err(Error::CommandFailed(format!("Failed to clean IP: {}", err))); }
        }
        let _ = fs::remove_file("/tmp/provisioner_hostapd.conf").await;
        Ok(())
    }

    pub async fn connect_impl(&self, ssid: &str, password: &str) -> Result<()> {
        self.stop_ap().await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let output = Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("add_network").output().await?;
        if !output.status.success() { return Err(Error::CommandFailed("add_network failed".into())); }
        let id_str = String::from_utf8_lossy(&output.stdout);
        let network_id: u32 = id_str.trim().parse().map_err(|_| Error::CommandFailed(format!("Invalid network id: {}", id_str)))?;
        let ssid_arg = format!("\"{}\"", ssid);
        Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("set_network").arg(network_id.to_string()).arg("ssid").arg(&ssid_arg).status().await?;
        if password.is_empty() {
            Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("set_network").arg(network_id.to_string()).arg("key_mgmt").arg("NONE").status().await?;
        } else {
            let psk_arg = format!("\"{}\"", password);
            Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("set_network").arg(network_id.to_string()).arg("psk").arg(&psk_arg).status().await?;
        }
        Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("enable_network").arg(network_id.to_string()).status().await?;
        for _ in 0..30 {
            let status_output = Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("status").output().await?;
            if !status_output.status.success() { return Err(Error::CommandFailed("status failed".into())); }
            let status_str = String::from_utf8_lossy(&status_output.stdout);
            if status_str.contains("wpa_state=COMPLETED") {
                Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("save_config").status().await?;
                let _ = Command::new("udhcpc").arg("-i").arg(IFACE_NAME).spawn();
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                return Ok(());
            }
            if status_str.contains("reason=WRONG_KEY") {
                Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("remove_network").arg(network_id.to_string()).status().await?;
                let networks = self.scan_internal().await.unwrap_or_default();
                *self.last_scan.lock().await = Some(networks);
                let _ = self.start_ap().await;
                return Err(Error::CommandFailed("Invalid password".into()));
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
        Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("remove_network").arg(network_id.to_string()).status().await?;
        let networks = self.scan_internal().await.unwrap_or_default();
        *self.last_scan.lock().await = Some(networks);
        let _ = self.start_ap().await;
        Err(Error::CommandFailed("Connection timed out".into()))
    }
}

#[async_trait]
impl PolicyCheck for WpaDbusTdmBackend {
    async fn is_connected(&self) -> Result<bool> {
        let output = Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("status").output().await;
        match output {
            Ok(out) => {
                if !out.status.success() { return Ok(false); }
                let stdout = String::from_utf8_lossy(&out.stdout);
                if stdout.contains("wpa_state=COMPLETED") && stdout.contains("ip_address=") { Ok(true) } else { Ok(false) }
            }
            Err(_) => Ok(false),
        }
    }
}

#[async_trait]
impl TdmBackend for WpaDbusTdmBackend {
    fn get_ap_config(&self) -> ApConfig { self.ap_config.as_ref().clone() }

    async fn enter_provisioning_mode_with_scan(&self) -> Result<Vec<Network>> {
        self.enter_with_scan_impl().await
    }

    async fn connect(&self, req: &ConnectionRequest) -> Result<()> {
        self.connect_impl(&req.ssid, &req.password).await
    }

    async fn exit_provisioning_mode(&self) -> Result<()> {
        self.stop_ap().await
    }
}
