use crate::traits::{Network, ProvisioningBackend};
use crate::{Error, Result};
use async_trait::async_trait;
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
        
        // 1. Set IP
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
            } else {
                tracing::warn!("IP address {} already exists on {}, proceeding.", AP_IP_ADDR, IFACE_NAME);
            }
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
        
        // Clean up IP
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
        println!("📡 [DbusBackend] Provisioning mode exited.");
        Ok(())
    }

    async fn scan(&self) -> Result<Vec<Network>> {
        println!("📡 [DbusBackend] Scanning for networks via wpa_cli...");

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
                    "wpa_supplicant service is not running or not accessible".to_string(),
                ));
            }
            if error_msg.contains("rfkill") {
                 return Err(Error::CommandFailed(
                    "Scan failed, device is blocked by rfkill".to_string(),
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
