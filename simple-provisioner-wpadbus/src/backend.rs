use crate::config::ap_config_from_toml_str;
use crate::structs::{ApConfig, ConnectionRequest, Network};
use anyhow::{anyhow, Result};
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

// 从配置文件加载 AP 配置
static GLOBAL_AP_CONFIG: Lazy<ApConfig> = Lazy::new(|| {
    const CONFIG_TOML: &str = include_str!("../config/wpa_dbus.toml");
    ap_config_from_toml_str(CONFIG_TOML)
});

const IFACE_NAME: &str = "wlan0";

// D-Bus 常量
const WPA_SUPPLICANT_SERVICE: &str = "fi.w1.wpa_supplicant1";
const WPA_SUPPLICANT_PATH: &str = "/fi/w1/wpa_supplicant1";
const WPA_SUPPLICANT_INTERFACE: &str = "fi.w1.wpa_supplicant1";

/// wpa_supplicant D-Bus 后端实现
#[derive(Debug)]
pub struct WpaDbusBackend {
    ap_config: Arc<ApConfig>,
    hostapd: Arc<Mutex<Option<tokio::process::Child>>>,
    dnsmasq: Arc<Mutex<Option<tokio::process::Child>>>,
    conn: Arc<Mutex<Option<Connection>>>,
}

impl WpaDbusBackend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            ap_config: Arc::new(GLOBAL_AP_CONFIG.clone()),
            hostapd: Arc::new(Mutex::new(None)),
            dnsmasq: Arc::new(Mutex::new(None)),
            conn: Arc::new(Mutex::new(None)),
        })
    }

    pub fn ap_config(&self) -> Arc<ApConfig> {
        self.ap_config.clone()
    }

    /// 确保 D-Bus 连接存在
    async fn ensure_conn(&self) -> Result<Connection> {
        if let Some(c) = self.conn.lock().await.clone() {
            return Ok(c);
        }
        let c = Connection::system()
            .await
            .map_err(|e| anyhow!("DBus connect failed: {}", e))?;
        *self.conn.lock().await = Some(c.clone());
        Ok(c)
    }

    /// 获取根 DBus 代理
    async fn root_proxy(&self) -> Result<Proxy<'_>> {
        let conn = self.ensure_conn().await?;
        Proxy::new(
            &conn,
            WPA_SUPPLICANT_SERVICE,
            WPA_SUPPLICANT_PATH,
            WPA_SUPPLICANT_INTERFACE,
        )
        .await
        .map_err(|e| anyhow!("proxy create error: {}", e))
    }

    /// DBus Value 转换辅助函数
    #[inline]
    fn ov<'a, V>(v: V) -> OwnedValue
    where
        V: Into<Value<'a>>,
    {
        v.into().try_into().unwrap()
    }

    /// 确保 wpa_supplicant 接口路径
    async fn ensure_iface_path(&self) -> Result<OwnedObjectPath> {
        let mgr = self.root_proxy().await?;
        let result = mgr.call_method("GetInterface", &(IFACE_NAME,)).await;
        if result.is_ok() {
            let reply = result.unwrap();
            let path: OwnedObjectPath = reply
                .body()
                .deserialize()
                .map_err(|e| anyhow!("GetInterface decode failed: {}", e))?;
            return Ok(path);
        }

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
            .map_err(|e| anyhow!("GetInterface failed after daemon startup: {}", e))?;
        let path: OwnedObjectPath = reply
            .body()
            .deserialize()
            .map_err(|e| anyhow!("GetInterface decode failed: {}", e))?;
        Ok(path)
    }

    /// 内部扫描方法
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
        .map_err(|e| anyhow!("iface proxy error: {}", e))?;

        let mut scan_done_stream = iface
            .receive_signal("ScanDone")
            .await
            .map_err(|e| anyhow!("Failed to listen for ScanDone: {}", e))?;

        // 触发扫描
        let opts: HashMap<String, OwnedValue> = HashMap::new();
        iface
            .call_method("Scan", &(opts))
            .await
            .map_err(|e| anyhow!("Scan failed: {}", e))?;

        let fut = async {
            if let Some(signal) = scan_done_stream.next().await {
                let (success,): (bool,) = signal
                    .body()
                    .deserialize()
                    .map_err(|e| anyhow!("Invalid ScanDone body: {}", e))?;
                if success {
                    return Ok(());
                }
            }
            Err(anyhow!("ScanDone signal not received or scan failed"))
        };

        match tokio::time::timeout(std::time::Duration::from_secs(15), fut).await {
            Ok(Ok(_)) => (),
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(anyhow!("Scan timed out")),
        }

        // 读取 BSS 列表
        let bss_paths: Vec<OwnedObjectPath> = iface
            .get_property::<Vec<OwnedObjectPath>>("BSSs")
            .await
            .map_err(|e| anyhow!("Get BSSs failed: {}", e))?;

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
            .map_err(|e| anyhow!("BSS proxy error: {}", e))?;

            // 获取 SSID
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

            // 获取信号强度
            let signal_dbm: i16 = bss.get_property::<i16>("Signal").await.unwrap_or(-100);

            // 获取安全信息
            let wpa: HashMap<String, OwnedValue> = bss.get_property("WPA").await.unwrap_or_default();
            let rsn: HashMap<String, OwnedValue> = bss.get_property("RSN").await.unwrap_or_default();

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

    /// 启动 AP 模式
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
                return Err(anyhow!("Failed to set IP: {}", err));
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

    /// 停止 AP 模式
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
                return Err(anyhow!("Failed to clean IP: {}", err));
            }
        }

        let _ = fs::remove_file("/tmp/provisioner_hostapd.conf").await;
        Ok(())
    }

    /// 公共方法：扫描并启动 AP（TDM 模式）
    pub async fn setup_and_scan(&self) -> Result<Vec<Network>> {
        let networks = self.scan_internal().await?;
        if networks.is_empty() {
            return Err(anyhow!("Initial scan returned no networks"));
        }
        self.start_ap().await?;
        Ok(networks)
    }

    /// 公共方法：连接到指定网络
    pub async fn connect(&self, req: &ConnectionRequest) -> Result<()> {
        // 停止 AP
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
        .map_err(|e| anyhow!("iface proxy error: {}", e))?;

        // 构建网络设置
        let mut net: HashMap<String, OwnedValue> = HashMap::new();
        net.insert("ssid".into(), Self::ov(req.ssid.as_bytes().to_vec()));
        if req.password.is_empty() {
            net.insert("key_mgmt".into(), Self::ov("NONE"));
        } else {
            net.insert("key_mgmt".into(), Self::ov("WPA-PSK"));
            net.insert("psk".into(), Self::ov(req.password.to_string()));
        }

        // AddNetwork
        let reply = iface
            .call_method("AddNetwork", &(net))
            .await
            .map_err(|e| anyhow!("AddNetwork failed: {}", e))?;
        let net_path: OwnedObjectPath = reply
            .body()
            .deserialize()
            .map_err(|e| anyhow!("AddNetwork decode failed: {}", e))?;

        // SelectNetwork
        let _ = iface
            .call_method("SelectNetwork", &(net_path.as_ref(),))
            .await
            .map_err(|e| anyhow!("SelectNetwork failed: {}", e))?;

        let mut props_stream = iface
            .receive_signal("PropertiesChanged")
            .await
            .map_err(|e| anyhow!("Failed to listen for PropertiesChanged: {}", e))?;

        let fut = async {
            while let Some(signal) = props_stream.next().await {
                match signal
                    .body()
                    .deserialize::<(String, HashMap<String, Value>, Vec<String>)>()
                {
                    Ok((iface_name, changed_props, _invalidated_props)) => {
                        if iface_name == "fi.w1.wpa_supplicant1.Interface" {
                            if let Some(state) = changed_props.get("State") {
                                if let Ok(state_str) = <&str>::try_from(state) {
                                    if state_str == "completed" {
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        return Err(anyhow!("Invalid PropertiesChanged body: {}", e));
                    }
                }
            }
            Err(anyhow!("PropertiesChanged stream ended unexpectedly"))
        };

        match tokio::time::timeout(std::time::Duration::from_secs(30), fut).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                // 超时：清理网络并尝试恢复 AP
                let _ = iface.call_method("RemoveNetwork", &(net_path.as_ref(),)).await;
                let _ = self.start_ap().await;
                Err(anyhow!("Connection timed out"))
            }
        }
    }
}
