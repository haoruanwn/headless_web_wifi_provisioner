use crate::traits::{Network, ProvisioningBackend};
use crate::{Error, Result};
use async_trait::async_trait;
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::{Arc, Mutex};
use tokio::process::{Child, Command};
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedValue};
use zbus_macros::proxy;

const IFACE_NAME: &str = "wlan0";
const AP_IP_ADDR: &str = "192.168.4.1/24";
const WPA_S_SERVICE: &str = "fi.w1.wpa_supplicant1";
const WPA_S_PATH: &str = "/fi/w1/wpa_supplicant1";

// Using zbus_macros to generate async proxy code for the interfaces we need.
#[proxy(interface = "org.freedesktop.DBus.Properties")]
trait Properties {
    // Return owned values to avoid lifetime issues inside the macro-generated code.
    fn get_all(&self, interface_name: &str) -> zbus::Result<HashMap<String, OwnedValue>>;
}

#[proxy(interface = "fi.w1.wpa_supplicant1")]
trait WpaSupplicant {
    #[zbus(property)]
    fn interfaces(&self) -> zbus::Result<Vec<String>>;
}

#[proxy(interface = "fi.w1.wpa_supplicant1.Interface")]
trait WpaInterface {
    fn scan(&self, args: HashMap<&str, &str>) -> zbus::Result<()>;
    fn add_network(&self, args: HashMap<String, OwnedValue>) -> zbus::Result<String>;
    fn select_network(&self, path: &str) -> zbus::Result<()>;

    #[zbus(property)]
    fn bsss(&self) -> zbus::Result<Vec<String>>;

    #[zbus(signal)]
    fn scan_done(&self, success: bool) -> zbus::Result<()>;
}

/// A backend that communicates with wpa_supplicant via D-Bus.
#[derive(Debug)]
pub struct DbusBackend {
    hostapd_pid: Arc<Mutex<Option<u32>>>,
    dnsmasq: Arc<Mutex<Option<Child>>>,
    connection: Connection,
}

impl DbusBackend {
    pub async fn new() -> Result<Self> {
        let connection = Connection::system().await?;
        Ok(Self {
            hostapd_pid: Arc::new(Mutex::new(None)),
            dnsmasq: Arc::new(Mutex::new(None)),
            connection,
        })
    }

    /// Finds the D-Bus object path for our wireless interface (e.g., wlan0)
    async fn get_iface_proxy(&self) -> Result<WpaInterfaceProxy<'_>> {
        let supplicant_proxy =
            WpaSupplicantProxy::new(&self.connection, WPA_S_SERVICE, WPA_S_PATH).await?;
        let iface_paths = supplicant_proxy.interfaces().await?;

        for path in iface_paths {
            // PropertiesProxy expects an object path; try converting string path into ObjectPath
            let obj_path = ObjectPath::try_from(path.as_str())?;
            let prop_proxy =
                PropertiesProxy::new(&self.connection, WPA_S_SERVICE, &obj_path).await?;
            let props = prop_proxy
                .get_all("fi.w1.wpa_supplicant1.Interface")
                .await?;
            if let Some(val) = props.get("Ifname") {
                if let Ok(ifname) = <OwnedValue as TryInto<String>>::try_into(val.clone()) {
                    if ifname == IFACE_NAME {
                        // create interface proxy using an owned object path to avoid returning a reference to a local
                        return Ok(WpaInterfaceProxy::new(
                            &self.connection,
                            WPA_S_SERVICE,
                            obj_path.into_owned(),
                        )
                        .await?);
                    }
                }
            }
        }
        Err(Error::CommandFailed(format!(
            "Wi-Fi interface '{}' not found.",
            IFACE_NAME
        )))
    }
}

#[async_trait]
impl ProvisioningBackend for DbusBackend {
    async fn enter_provisioning_mode(&self) -> Result<()> {
        println!("📡 [DbusBackend] Entering provisioning mode...");
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
            return Err(Error::CommandFailed(format!(
                "Failed to set IP address: {}",
                error_msg
            )));
        }
        let child = Command::new("hostapd")
            .arg("/etc/hostapd.conf")
            .arg("-B")
            .spawn()?;
        if let Some(pid) = child.id() {
            println!("📡 [DbusBackend] Started hostapd with PID: {}", pid);
            *self.hostapd_pid.lock().unwrap() = Some(pid);
        } else {
            return Err(Error::CommandFailed(
                "Could not get PID for hostapd process".to_string(),
            ));
        }
        
        // Start dnsmasq process
        println!("📡 [DbusBackend] Starting dnsmasq...");
        let ap_ip_only = AP_IP_ADDR.split('/').next().unwrap_or("");
        let dnsmasq_child = Command::new("dnsmasq")
            .arg(format!("--interface={}", IFACE_NAME))
            .arg("--dhcp-range=192.168.4.100,192.168.4.200,12h")
            .arg(format!("--address=/#/{}", ap_ip_only))
            .arg("--no-resolv")
            .arg("--no-hosts")
            .arg("--no-daemon")
            .spawn()?;
        
        *self.dnsmasq.lock().unwrap() = Some(dnsmasq_child);

        Ok(())
    }

    async fn exit_provisioning_mode(&self) -> Result<()> {
        println!("📡 [DbusBackend] Exiting provisioning mode...");

        // Stop dnsmasq process
        println!("📡 [DbusBackend] Stopping dnsmasq...");
        let dnsmasq_child_to_kill = self.dnsmasq.lock().unwrap().take();
        if let Some(mut child) = dnsmasq_child_to_kill {
            if let Err(e) = child.kill().await {
                tracing::warn!("Failed to kill dnsmasq process: {}", e);
            }
        }

        let pid_to_kill = { *self.hostapd_pid.lock().unwrap() };

        if let Some(pid) = pid_to_kill {
            println!("📡 [DbusBackend] Killing hostapd process with PID: {}", pid);
            let output = Command::new("kill").arg(pid.to_string()).output().await?;
            if !output.status.success() {
                eprintln!(
                    "Warning: Failed to kill hostapd process: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
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
            return Err(Error::CommandFailed(format!(
                "Failed to clean up IP address: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        println!("📡 [DbusBackend] Provisioning mode exited.");
        Ok(())
    }

    async fn scan(&self) -> Result<Vec<Network>> {
        println!("📡 [DbusBackend] Scanning for networks via D-Bus...");
        let iface_proxy = self.get_iface_proxy().await?;
        let mut scan_done_stream = iface_proxy.receive_scan_done().await?;

        iface_proxy.scan(HashMap::new()).await?;
        scan_done_stream.next().await;

        let bss_paths = iface_proxy.bsss().await?;
        let mut networks = Vec::new();

        for path in bss_paths {
            let obj_path = ObjectPath::try_from(path.as_str())?;
            let prop_proxy =
                PropertiesProxy::new(&self.connection, WPA_S_SERVICE, &obj_path).await?;
            let props = prop_proxy.get_all("fi.w1.wpa_supplicant1.BSS").await?;

            if let (Some(ssid_val), Some(signal_val)) = (props.get("SSID"), props.get("Signal")) {
                // Try to convert OwnedValue into Vec<u8> and i16 respectively
                if let Ok(bytes) = <OwnedValue as TryInto<Vec<u8>>>::try_into(ssid_val.clone()) {
                    if let Ok(ssid) = String::from_utf8(bytes) {
                        // Basic security detection
                        let security = if props.contains_key("RSN") {
                            "WPA2/3".to_string()
                        } else if props.contains_key("WPA") {
                            "WPA".to_string()
                        } else {
                            "Open".to_string()
                        };

                        if let Ok(signal) =
                            <OwnedValue as TryInto<i16>>::try_into(signal_val.clone())
                        {
                            // Convert signal from dBm to a rough 0-100 percentage.
                            let signal_percent = ((signal.clamp(-100i16, -50i16) + 100) * 2) as u8;

                            networks.push(Network {
                                ssid,
                                signal: signal_percent,
                                security,
                            });
                        }
                    }
                }
            }
        }
        Ok(networks)
    }

    async fn connect(&self, ssid: &str, password: &str) -> Result<()> {
        println!(
            "📡 [DbusBackend] Attempting to connect to SSID: '{}' via D-Bus...",
            ssid
        );
        let iface_proxy = self.get_iface_proxy().await?;

        let mut args = HashMap::new();
        // Construct OwnedValue via intermediate Value then try_into owned
        let ssid_val = zbus::zvariant::Value::new(ssid.as_bytes());
        let ssid_owned = OwnedValue::try_from(ssid_val)?;
        args.insert("ssid".to_string(), ssid_owned);
        if !password.is_empty() {
            let psk_val = zbus::zvariant::Value::new(password);
            let psk_owned = OwnedValue::try_from(psk_val)?;
            args.insert("psk".to_string(), psk_owned);
        }

        let net_path = iface_proxy.add_network(args).await?;
        iface_proxy.select_network(&net_path).await?;

        println!(
            "📡 [DbusBackend] Connection process initiated for '{}'",
            ssid
        );
        Ok(())
    }
}
