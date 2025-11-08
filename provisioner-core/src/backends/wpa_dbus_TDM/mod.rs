use crate::config::ap_config_from_toml_str;
use crate::traits::{ApConfig, ConnectionRequest, Network, PolicyCheck, TdmBackend};
use crate::{Error, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;
use tokio::fs;
use tokio::process::Command;
use tokio::sync::Mutex;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy};
use futures_util::stream::StreamExt;

// 通过 D-Bus 与 wpa_supplicant 进行交互的功能

// MVP DBus backend for wpa_supplicant + external hostapd/dnsmasq for AP mode.
// Station operations (scan/connect) will use DBus where feasible; fallback to wpa_cli textual parsing for now.

static GLOBAL_AP_CONFIG: Lazy<ApConfig> = Lazy::new(|| {
    const CONFIG_TOML: &str = include_str!("../../../../configs/wpa_dbus_tdm.toml");
    ap_config_from_toml_str(CONFIG_TOML)
});

const IFACE_NAME: &str = "wlan0";

// D-Bus constants for wpa_supplicant
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
        if let Some(c) = self.conn.lock().await.clone() {
            return Ok(c);
        }
        let c = Connection::system()
            .await
            .map_err(|e| Error::CommandFailed(format!("DBus connect failed: {}", e)))?;
        *self.conn.lock().await = Some(c.clone());
        Ok(c)
    }

    async fn root_proxy(&self) -> Result<Proxy<'_>> {
        let conn = self.ensure_conn().await?;
        Proxy::new(
            &conn,
            WPA_SUPPLICANT_SERVICE,
            WPA_SUPPLICANT_PATH,
            WPA_SUPPLICANT_INTERFACE,
        )
        .await
        .map_err(|e| Error::CommandFailed(format!("proxy create error: {}", e)))
    }

    #[inline]
    fn ov<'a, V>(v: V) -> OwnedValue
    where
        V: Into<Value<'a>>,
    {
        v.into().try_into().unwrap()
    }

    async fn ensure_iface_path(&self) -> Result<OwnedObjectPath> {
        let mgr = self.root_proxy().await?;
        let result = mgr.call_method("GetInterface", &(IFACE_NAME,)).await;
        if result.is_ok() {
            let reply = result.unwrap();
            let path: OwnedObjectPath = reply
                .body()
                .deserialize()
                .map_err(|e| Error::CommandFailed(format!("GetInterface decode failed: {}", e)))?;
            return Ok(path);
        }

        // 在这里用命令启动wpa_supplicant守护进程，这是必要的一部，因为D-Bus接口的可用性依赖于此
        // wpa_supplicant daemon not yet available via D-Bus, try to start it
        // This is a necessary precondition for D-Bus interface availability
        tracing::info!("wpa_supplicant D-Bus interface not available, attempting to start daemon...");
        let spawn_result = Command::new("wpa_supplicant")
            .arg("-B")
            .arg(format!("-i{}", IFACE_NAME))
            .arg("-c/etc/wpa_supplicant.conf")
            .spawn();
        
        match spawn_result {
            Ok(_) => {
                tracing::debug!("wpa_supplicant daemon started, waiting for D-Bus interface...");
            }
            Err(e) => {
                tracing::warn!("Failed to spawn wpa_supplicant: {}", e);
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let reply = mgr
            .call_method("GetInterface", &(IFACE_NAME,))
            .await
            .map_err(|e| Error::CommandFailed(format!("GetInterface failed after daemon startup: {}", e)))?;
        let path: OwnedObjectPath = reply
            .body()
            .deserialize()
            .map_err(|e| Error::CommandFailed(format!("GetInterface decode failed: {}", e)))?;
        Ok(path)
    }

    async fn scan_internal(&self) -> Result<Vec<Network>> {
        let iface_path = self.ensure_iface_path().await?;
        let conn = self.ensure_conn().await?;
        let iface = Proxy::new(
            &conn,
            WPA_SUPPLICANT_SERVICE,
            iface_path.as_ref(),
            "fi.w1.wpa_supplicant1.Interface",
        )
        .await
        .map_err(|e| Error::CommandFailed(format!("iface proxy error: {}", e)))?;

        let mut scan_done_stream = iface
            .receive_signal("ScanDone")
            .await
            .map_err(|e| Error::CommandFailed(format!("Failed to listen for ScanDone: {}", e)))?;

        // Trigger scan: Scan(a{sv}) with empty options
        let opts: HashMap<String, OwnedValue> = HashMap::new();
        iface
            .call_method("Scan", &(opts))
            .await
            .map_err(|e| Error::CommandFailed(format!("Scan failed: {}", e)))?;

        let fut = async {
            if let Some(signal) = scan_done_stream.next().await {
                let (success,): (bool,) = signal.body().deserialize().map_err(|e| Error::CommandFailed(format!("Invalid ScanDone body: {}", e)))?;
                if success {
                    return Ok(());
                }
            }
            Err(Error::CommandFailed("ScanDone signal not received or scan failed".into()))
        };

        match tokio::time::timeout(std::time::Duration::from_secs(15), fut).await {
            Ok(Ok(_)) => (),
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(Error::CommandFailed("Scan timed out".into())),
        }

        // Read BSS list
        let bss_paths: Vec<OwnedObjectPath> = iface
            .get_property::<Vec<OwnedObjectPath>>("BSSs")
            .await
            .map_err(|e| Error::CommandFailed(format!("Get BSSs failed: {}", e)))?;
        let conn = self.ensure_conn().await?;
        let mut networks = Vec::new();
        for bss_path in bss_paths {
            let bss = Proxy::new(
                &conn,
                WPA_SUPPLICANT_SERVICE,
                bss_path.as_ref(),
                "fi.w1.wpa_supplicant1.BSS",
            )
            .await
            .map_err(|e| Error::CommandFailed(format!("BSS proxy error: {}", e)))?;
            
            // Get SSID - skip this BSS if we can't get it
            let ssid_bytes = match bss.get_property::<Vec<u8>>("SSID").await {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!("Failed to get SSID for BSS {:?}: {}", bss_path, e);
                    continue;
                }
            };
            
            if ssid_bytes.is_empty() {
                continue;
            }
            
            // Get Signal strength - use a default if unavailable but log a warning
            let signal_dbm: i16 = match bss.get_property::<i16>("Signal").await {
                Ok(sig) => sig,
                Err(e) => {
                    tracing::warn!("Failed to get Signal for BSS {:?}: {}, using default -100", bss_path, e);
                    -100
                }
            };
            
            // Determine security from WPA/RSN presence - use defaults if unavailable
            let wpa: HashMap<String, OwnedValue> = match bss.get_property("WPA").await {
                Ok(w) => w,
                Err(e) => {
                    tracing::debug!("Failed to get WPA for BSS {:?}: {}", bss_path, e);
                    HashMap::new()
                }
            };
            
            let rsn: HashMap<String, OwnedValue> = match bss.get_property("RSN").await {
                Ok(r) => r,
                Err(e) => {
                    tracing::debug!("Failed to get RSN for BSS {:?}: {}", bss_path, e);
                    HashMap::new()
                }
            };
            
            let security = if !rsn.is_empty() {
                "WPA2".to_string()
            } else if !wpa.is_empty() {
                "WPA".to_string()
            } else {
                "Open".to_string()
            };
            
            let ssid = String::from_utf8(ssid_bytes.clone())
                .unwrap_or_else(|_| format!("{:X?}", ssid_bytes));
            let signal_percent = ((signal_dbm.clamp(-100, -50) + 100) * 2) as u8;
            networks.push(Network {
                ssid,
                signal: signal_percent,
                security,
            });
        }
        Ok(networks)
    }

    async fn enter_with_scan_impl(&self) -> Result<Vec<Network>> {
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

    // 启动 AP 模式，配置并启动 hostapd 和 dnsmasq
    async fn start_ap(&self) -> Result<()> {
        let _ = Command::new("killall")
            .arg("-9")
            .arg("hostapd")
            .arg("dnsmasq")
            .arg("wpa_supplicant")
            .status()
            .await;
        let output = Command::new("ip")
            .arg("addr")
            .arg("add")
            .arg(&self.ap_config.gateway_cidr)
            .arg("dev")
            .arg(IFACE_NAME)
            .output()
            .await?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            if !err.contains("File exists") {
                return Err(Error::CommandFailed(format!("Failed to set IP: {}", err)));
            }
        }
        let hostapd_conf = format!(
            "interface={}\nssid={}\nwpa=2\nwpa_passphrase={}\nhw_mode=g\nchannel=6\nwpa_key_mgmt=WPA-PSK\nwpa_pairwise=CCMP\nrsn_pairwise=CCMP\n",
            IFACE_NAME, self.ap_config.ssid, self.ap_config.psk
        );
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
        if let Some(mut child) = self.dnsmasq.lock().await.take() {
            let _ = child.kill().await;
        }
        if let Some(mut child) = self.hostapd.lock().await.take() {
            let _ = child.kill().await;
        }
        let output = Command::new("ip")
            .arg("addr")
            .arg("del")
            .arg(&self.ap_config.gateway_cidr)
            .arg("dev")
            .arg(IFACE_NAME)
            .output()
            .await?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            if !err.contains("Cannot assign requested address") {
                return Err(Error::CommandFailed(format!("Failed to clean IP: {}", err)));
            }
        }
        let _ = fs::remove_file("/tmp/provisioner_hostapd.conf").await;
        Ok(())
    }

    pub async fn connect_impl(&self, ssid: &str, password: &str) -> Result<()> {
        // Stop AP first
        let _ = self.stop_ap().await;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        let iface_path = self.ensure_iface_path().await?;
        let conn = self.ensure_conn().await?;
        let iface = Proxy::new(
            &conn,
            WPA_SUPPLICANT_SERVICE,
            iface_path.as_ref(),
            "fi.w1.wpa_supplicant1.Interface",
        )
        .await
        .map_err(|e| Error::CommandFailed(format!("iface proxy error: {}", e)))?;

        // Build network settings a{sv}
        let mut net: HashMap<String, OwnedValue> = HashMap::new();
        net.insert("ssid".into(), Self::ov(ssid.as_bytes().to_vec()));
        if password.is_empty() {
            net.insert("key_mgmt".into(), Self::ov("NONE"));
        } else {
            net.insert("key_mgmt".into(), Self::ov("WPA-PSK"));
            net.insert("psk".into(), Self::ov(password.to_string()));
        }

        // AddNetwork -> object path
        let reply = iface
            .call_method("AddNetwork", &(net))
            .await
            .map_err(|e| Error::CommandFailed(format!("AddNetwork failed: {}", e)))?;
        let net_path: OwnedObjectPath = reply
            .body()
            .deserialize()
            .map_err(|e| Error::CommandFailed(format!("AddNetwork decode failed: {}", e)))?;

        // SelectNetwork
        let _ = iface
            .call_method("SelectNetwork", &(net_path.as_ref(),))
            .await
            .map_err(|e| Error::CommandFailed(format!("SelectNetwork failed: {}", e)))?;

        let mut props_stream = iface
            .receive_signal("PropertiesChanged")
            .await
            .map_err(|e| Error::CommandFailed(format!("Failed to listen for PropertiesChanged: {}", e)))?;

        let fut = async {
            while let Some(signal) = props_stream.next().await {
                match signal
                    .body().deserialize::<(String, HashMap<String, Value>, Vec<String>)>()
                {
                    Ok((iface_name, changed_props, _invalidated_props)) => {
                        if iface_name == "fi.w1.wpa_supplicant1.Interface" {
                            if let Some(state) = changed_props.get("State") {
                                if let Ok(state_str) = <&str>::try_from(state) {
                                    if state_str == "completed" {
                                        // Connection successful. L3 IP address acquisition is delegated to
                                        // the system's network service (systemd-networkd, NetworkManager, etc.)
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        return Err(Error::CommandFailed(format!("Invalid PropertiesChanged body: {}", e)));
                    }
                }
            }
            Err(Error::CommandFailed("PropertiesChanged stream ended unexpectedly".into()))
        };

        match tokio::time::timeout(std::time::Duration::from_secs(30), fut).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                // Timeout: clean network and restore AP list
                let _ = iface.call_method("RemoveNetwork", &(net_path.as_ref(),)).await;
                let networks = self.scan_internal().await.unwrap_or_default();
                *self.last_scan.lock().await = Some(networks);
                let _ = self.start_ap().await;
                Err(Error::CommandFailed("Connection timed out".into()))
            }
        }
    }
}

#[async_trait]
impl PolicyCheck for WpaDbusTdmBackend {
    async fn is_connected(&self) -> Result<bool> {
        // Check via DBus Interface.State
        match self.ensure_iface_path().await {
            Ok(iface_path) => {
                let conn = self.ensure_conn().await?;
                let iface = Proxy::new(
                    &conn,
                    WPA_SUPPLICANT_SERVICE,
                    iface_path.as_ref(),
                    "fi.w1.wpa_supplicant1.Interface",
                )
                .await
                .map_err(|e| Error::CommandFailed(format!("iface proxy error: {}", e)))?;
                let state: String = iface
                    .get_property("State")
                    .await
                    .unwrap_or_else(|_| "disconnected".into());
                Ok(state.to_lowercase() == "completed")
            }
            Err(_) => Ok(false),
        }
    }
}

#[async_trait]
impl TdmBackend for WpaDbusTdmBackend {
    fn get_ap_config(&self) -> ApConfig {
        self.ap_config.as_ref().clone()
    }

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