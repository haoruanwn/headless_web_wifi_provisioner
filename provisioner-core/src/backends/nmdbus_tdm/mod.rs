use crate::config::ap_config_from_toml_str;
use crate::traits::{ApConfig, ConnectionRequest, Network, PolicyCheck, TdmBackend};
use crate::{Error, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy};
use zbus::proxy::SignalStream;
use futures_util::stream::StreamExt;

// This backend is a stub showcasing DBus interaction with NetworkManager for scanning & connecting.
// AP mode still leverages nmcli commands for simplicity; later we can move those to pure D-Bus calls.

static GLOBAL_AP_CONFIG: Lazy<ApConfig> = Lazy::new(|| {
    const CONFIG_TOML: &str = include_str!("../../../../configs/nmdbus_tdm.toml");
    ap_config_from_toml_str(CONFIG_TOML)
});

const IFACE_NAME: &str = "wlan0";
const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_IFACE: &str = "org.freedesktop.NetworkManager";

#[derive(Debug)]
pub struct NmdbusTdmBackend {
    ap_config: Arc<ApConfig>,
    last_scan: Arc<Mutex<Option<Vec<Network>>>>,
    // Hold a zbus connection (lazy-init on first use for now)
    conn: Arc<Mutex<Option<Connection>>>,
    // Tracks the active AP connection & active-connection object for cleanup
    active_ap_con: Arc<Mutex<Option<OwnedObjectPath>>>,
    active_ap_ac: Arc<Mutex<Option<OwnedObjectPath>>>,
}

impl NmdbusTdmBackend {
    #[inline]
    fn ov<'a, V>(v: V) -> OwnedValue
    where
        V: Into<Value<'a>>,
    {
        v.into().try_into().unwrap()
    }
    pub fn new() -> Result<Self> {
        Ok(Self {
            ap_config: Arc::new(GLOBAL_AP_CONFIG.clone()),
            last_scan: Arc::new(Mutex::new(None)),
            conn: Arc::new(Mutex::new(None)),
            active_ap_con: Arc::new(Mutex::new(None)),
            active_ap_ac: Arc::new(Mutex::new(None)),
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

    async fn scan_internal(&self) -> Result<Vec<Network>> {
        // Pure DBus flow: GetDevices -> pick wireless device -> RequestScan -> GetAccessPoints -> read AP properties
        let conn = self.ensure_conn().await?;
        let dpath = self.get_wifi_device_path().await?;

        // Wireless-specific proxy
        let wifi = Proxy::new(
            &conn,
            NM_SERVICE,
            dpath.as_ref(),
            "org.freedesktop.NetworkManager.Device.Wireless",
        )
        .await
        .map_err(|e| Error::CommandFailed(format!("Wireless proxy error: {}", e)))?;

        let mut scan_done_stream = wifi
            .receive_signal("ScanDone")
            .await
            .map_err(|e| Error::CommandFailed(format!("Failed to listen for ScanDone: {}", e)))?;

        // RequestScan with empty options
        let opts: HashMap<String, OwnedValue> = HashMap::new();
        wifi
            .call_method("RequestScan", &(opts))
            .await
            .map_err(|e| Error::CommandFailed(format!("RequestScan failed: {}", e)))?;

        let fut = async {
            scan_done_stream.next().await;
            Ok::<(), Error>(())
        };

        if let Err(_) = tokio::time::timeout(std::time::Duration::from_secs(15), fut).await {
            return Err(Error::CommandFailed("Scan timed out".into()));
        }

        // GetAccessPoints -> Vec<ObjectPath>
        let msg = wifi
            .call_method("GetAccessPoints", &())
            .await
            .map_err(|e| Error::CommandFailed(format!("GetAccessPoints failed: {}", e)))?;
        let aps: Vec<OwnedObjectPath> = msg
            .body()
            .deserialize()
            .map_err(|e| Error::CommandFailed(format!("Decode AccessPoints failed: {}", e)))?;

        // Read properties from each AP
        let mut networks = Vec::new();
        for ap_path in aps {
            let ap = Proxy::new(
                &conn,
                NM_SERVICE,
                ap_path.as_ref(),
                "org.freedesktop.NetworkManager.AccessPoint",
            )
            .await
            .map_err(|e| Error::CommandFailed(format!("AP proxy error: {}", e)))?;
            let ssid_bytes: Vec<u8> = ap
                .get_property::<Vec<u8>>("Ssid")
                .await
                .map_err(|e| Error::CommandFailed(format!("Get Ssid failed: {}", e)))?;
            let strength: u8 = ap
                .get_property::<u8>("Strength")
                .await
                .map_err(|e| Error::CommandFailed(format!("Get Strength failed: {}", e)))?;
            let wpa: u32 = ap.get_property::<u32>("WpaFlags").await.unwrap_or(0);
            let rsn: u32 = ap.get_property::<u32>("RsnFlags").await.unwrap_or(0);
            let security = if rsn != 0 {
                "WPA2"
            } else if wpa != 0 {
                "WPA"
            } else {
                "Open"
            }
            .to_string();
            let ssid = String::from_utf8(ssid_bytes.clone()).unwrap_or_else(|_| {
                // fallback: hex encode if non-utf8
                format!("{:X?}", ssid_bytes)
            });
            networks.push(Network {
                ssid,
                signal: strength,
                security,
            });
        }
        Ok(networks)
    }

    // Helper: pick a wireless device (prefer IFACE_NAME)
    async fn get_wifi_device_path(&self) -> Result<OwnedObjectPath> {
        let conn = self.ensure_conn().await?;
        let nm = Proxy::new(&conn, NM_SERVICE, NM_PATH, NM_IFACE)
            .await
            .map_err(|e| Error::CommandFailed(format!("Proxy create error: {}", e)))?;
        let msg = nm
            .call_method("GetDevices", &())
            .await
            .map_err(|e| Error::CommandFailed(format!("GetDevices call failed: {}", e)))?;
        let devices: Vec<OwnedObjectPath> = msg
            .body()
            .deserialize()
            .map_err(|e| Error::CommandFailed(format!("GetDevices decode failed: {}", e)))?;
        let mut chosen: Option<OwnedObjectPath> = None;
        for dpath in devices {
            let dev = Proxy::new(
                &conn,
                NM_SERVICE,
                dpath.as_ref(),
                "org.freedesktop.NetworkManager.Device",
            )
            .await
            .map_err(|e| Error::CommandFailed(format!("Device proxy error: {}", e)))?;
            let dtype: u32 = dev
                .get_property::<u32>("DeviceType")
                .await
                .map_err(|e| Error::CommandFailed(format!("Get DeviceType failed: {}", e)))?;
            if dtype != 2 {
                continue;
            }
            let ifname: String = dev
                .get_property::<String>("Interface")
                .await
                .map_err(|e| Error::CommandFailed(format!("Get Interface failed: {}", e)))?;
            if ifname == IFACE_NAME {
                return Ok(dpath);
            }
            if chosen.is_none() {
                chosen = Some(dpath);
            }
        }
        chosen.ok_or_else(|| Error::CommandFailed("No wireless device found".into()))
    }

    async fn enter_with_scan_impl(&self) -> Result<Vec<Network>> {
        let networks = self.scan_internal().await?;
        if networks.is_empty() {
            return Err(Error::CommandFailed(
                "Initial scan returned no networks".into(),
            ));
        }
        *self.last_scan.lock().await = Some(networks.clone());
        // Use nmcli to set up AP hotspot similar to nmcli backend for now.
        self.start_ap().await?;
        Ok(networks)
    }

    async fn start_ap(&self) -> Result<()> {
        // Build AddAndActivateConnection settings for AP + shared IPv4 + custom address
        let device_path = self.get_wifi_device_path().await?;
        let conn = self.ensure_conn().await?;
        let nm = Proxy::new(&conn, NM_SERVICE, NM_PATH, NM_IFACE)
            .await
            .map_err(|e| Error::CommandFailed(format!("Proxy create error: {}", e)))?;

        // connection setting
        let mut s_connection: HashMap<String, OwnedValue> = HashMap::new();
        s_connection.insert("id".into(), Self::ov(self.ap_config.ssid.clone()));
        s_connection.insert("type".into(), Self::ov("802-11-wireless"));
        s_connection.insert("autoconnect".into(), Self::ov(false));
        s_connection.insert("interface-name".into(), Self::ov(IFACE_NAME));

        // wireless setting
        let mut s_wifi: HashMap<String, OwnedValue> = HashMap::new();
        s_wifi.insert("mode".into(), Self::ov("ap"));
        s_wifi.insert(
            "ssid".into(),
            Self::ov(self.ap_config.ssid.as_bytes().to_vec()),
        );

        // security
        let mut s_sec: HashMap<String, OwnedValue> = HashMap::new();
        s_sec.insert("key-mgmt".into(), Self::ov("wpa-psk"));
        s_sec.insert("psk".into(), Self::ov(self.ap_config.psk.clone()));

        // ipv4 setting: shared + address-data [{address, prefix}]
        let mut s_ipv4: HashMap<String, OwnedValue> = HashMap::new();
        s_ipv4.insert("method".into(), Self::ov("shared"));
        let mut addr_data_entry: HashMap<String, OwnedValue> = HashMap::new();
        let (addr, prefix) = match self.ap_config.gateway_cidr.split_once('/') {
            Some((a, p)) => (a.to_string(), p.parse::<u32>().unwrap_or(24)),
            None => (self.ap_config.gateway_cidr.clone(), 24),
        };
        addr_data_entry.insert("address".into(), Self::ov(addr));
        addr_data_entry.insert("prefix".into(), Self::ov(prefix));
        let address_data: Vec<HashMap<String, OwnedValue>> = vec![addr_data_entry];
        s_ipv4.insert("address-data".into(), Self::ov(address_data));

        // ipv6 ignored
        let mut s_ipv6: HashMap<String, OwnedValue> = HashMap::new();
        s_ipv6.insert("method".into(), Self::ov("ignore"));

        // root settings dict
        let mut settings: HashMap<String, HashMap<String, OwnedValue>> = HashMap::new();
        settings.insert("connection".into(), s_connection);
        settings.insert("802-11-wireless".into(), s_wifi);
        settings.insert("802-11-wireless-security".into(), s_sec);
        settings.insert("ipv4".into(), s_ipv4);
        settings.insert("ipv6".into(), s_ipv6);

        let specific = ObjectPath::try_from("/")
            .map_err(|e| Error::CommandFailed(format!("Invalid object path: {}", e)))?;
        let reply = nm
            .call_method(
                "AddAndActivateConnection",
                &(settings, device_path.as_ref(), specific.as_ref()),
            )
            .await
            .map_err(|e| Error::CommandFailed(format!("AddAndActivateConnection failed: {}", e)))?;
        let (con_path, ac_path, _dev_path): (OwnedObjectPath, OwnedObjectPath, OwnedObjectPath) =
            reply.body().deserialize().map_err(|e| {
                Error::CommandFailed(format!("AddAndActivate decode failed: {}", e))
            })?;
        *self.active_ap_con.lock().await = Some(con_path);
        *self.active_ap_ac.lock().await = Some(ac_path);
        Ok(())
    }

    async fn stop_ap(&self) -> Result<()> {
        let conn = self.ensure_conn().await?;
        let nm = Proxy::new(&conn, NM_SERVICE, NM_PATH, NM_IFACE)
            .await
            .map_err(|e| Error::CommandFailed(format!("Proxy create error: {}", e)))?;
        if let Some(ac_path) = self.active_ap_ac.lock().await.take() {
            let _ = nm
                .call_method("DeactivateConnection", &(ac_path.as_ref(),))
                .await;
        }
        if let Some(con_path) = self.active_ap_con.lock().await.take() {
            // delete the connection profile to avoid leftover
            let con = Proxy::new(
                &conn,
                NM_SERVICE,
                con_path.as_ref(),
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await
            .map_err(|e| Error::CommandFailed(format!("Settings.Connection proxy error: {}", e)))?;
            let _ = con.call_method("Delete", &()).await;
        }
        Ok(())
    }

    pub async fn connect_impl(&self, ssid: &str, password: &str) -> Result<()> {
        // Ensure AP is stopped first
        let _ = self.stop_ap().await;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let device_path = self.get_wifi_device_path().await?;
        let conn = self.ensure_conn().await?;
        let nm = Proxy::new(&conn, NM_SERVICE, NM_PATH, NM_IFACE)
            .await
            .map_err(|e| Error::CommandFailed(format!("Proxy create error: {}", e)))?;

        // connection setting
        let mut s_connection: HashMap<String, OwnedValue> = HashMap::new();
        s_connection.insert("id".into(), Self::ov("ProvisionerSTA"));
        s_connection.insert("type".into(), Self::ov("802-11-wireless"));
        s_connection.insert("autoconnect".into(), Self::ov(false));
        s_connection.insert("interface-name".into(), Self::ov(IFACE_NAME));

        // wireless setting (infrastructure is default)
        let mut s_wifi: HashMap<String, OwnedValue> = HashMap::new();
        s_wifi.insert("ssid".into(), Self::ov(ssid.as_bytes().to_vec()));

        // security (optional)
        let mut s_sec: HashMap<String, OwnedValue> = HashMap::new();
        if password.is_empty() {
            s_sec.insert("key-mgmt".into(), Self::ov("none"));
        } else {
            s_sec.insert("key-mgmt".into(), Self::ov("wpa-psk"));
            s_sec.insert("psk".into(), Self::ov(password.to_string()));
        }

        // IPv4 auto
        let mut s_ipv4: HashMap<String, OwnedValue> = HashMap::new();
        s_ipv4.insert("method".into(), Self::ov("auto"));
        // IPv6 ignore or auto (choose ignore to be conservative)
        let mut s_ipv6: HashMap<String, OwnedValue> = HashMap::new();
        s_ipv6.insert("method".into(), Self::ov("ignore"));

        let mut settings: HashMap<String, HashMap<String, OwnedValue>> = HashMap::new();
        settings.insert("connection".into(), s_connection);
        settings.insert("802-11-wireless".into(), s_wifi);
        if !s_sec.is_empty() {
            settings.insert("802-11-wireless-security".into(), s_sec);
        }
        settings.insert("ipv4".into(), s_ipv4);
        settings.insert("ipv6".into(), s_ipv6);

        let specific = ObjectPath::try_from("/")
            .map_err(|e| Error::CommandFailed(format!("Invalid object path: {}", e)))?;
        let reply = nm
            .call_method(
                "AddAndActivateConnection",
                &(settings, device_path.as_ref(), specific.as_ref()),
            )
            .await
            .map_err(|e| Error::CommandFailed(format!("AddAndActivateConnection failed: {}", e)))?;
        let (_con_path, ac_path, _dev_path): (OwnedObjectPath, OwnedObjectPath, OwnedObjectPath) =
            reply
                .body()
                .deserialize()
                .map_err(|e| Error::CommandFailed(format!("AddAndActivate decode failed: {}", e)))?;
        
        let ac_proxy = Proxy::new(
            &conn,
            NM_SERVICE,
            ac_path.as_ref(),
            "org.freedesktop.NetworkManager.Connection.Active",
        )
        .await
        .map_err(|e| Error::CommandFailed(format!("Active connection proxy error: {}", e)))?;

        let mut state_stream = ac_proxy
            .receive_signal("StateChanged")
            .await
            .map_err(|e| Error::CommandFailed(format!("Failed to listen for StateChanged: {}", e)))?;

        let fut = async {
            while let Some(signal) = state_stream.next().await {
                let (state, _reason): (u32, u32) = signal
                    .body()
                    .deserialize()
                    .map_err(|e| Error::CommandFailed(format!("Invalid StateChanged body: {}", e)))?;
                match state {
                    2 => return Ok(()), // NM_ACTIVE_CONNECTION_STATE_ACTIVATED
                    4 => return Err(Error::CommandFailed("Connection failed (deactivated)".into())),
                    _ => continue,
                }
            }
            Err(Error::CommandFailed("Connection state stream ended unexpectedly".into()))
        };

        match tokio::time::timeout(std::time::Duration::from_secs(30), fut).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::CommandFailed("Connection timed out".into())),
        }
    }
}

#[async_trait]
impl PolicyCheck for NmdbusTdmBackend {
    async fn is_connected(&self) -> Result<bool> {
        let conn = self.ensure_conn().await?;
        let nm = Proxy::new(&conn, NM_SERVICE, NM_PATH, NM_IFACE)
            .await
            .map_err(|e| Error::CommandFailed(format!("Proxy create error: {}", e)))?;
        let state: u32 = nm
            .get_property("State")
            .await
            .map_err(|e| Error::CommandFailed(format!("Failed to get NM state: {}", e)))?;
        Ok(state == 70) // NM_STATE_CONNECTED
    }
}

#[async_trait]
impl TdmBackend for NmdbusTdmBackend {
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
