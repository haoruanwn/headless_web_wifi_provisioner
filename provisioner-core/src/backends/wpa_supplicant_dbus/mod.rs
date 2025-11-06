use crate::traits::{Network, ConcurrentBackend, ProvisioningTerminator};
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

// Clean, single-definition D-Bus backend implementation
// Clean, single-definition D-Bus backend implementation
use crate::traits::{Network, ConcurrentBackend, ProvisioningTerminator};
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

#[proxy(interface = "org.freedesktop.DBus.Properties")]
trait Properties {
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

    async fn get_iface_proxy(&self) -> Result<WpaInterfaceProxy<'_>> {
        use crate::traits::{Network, ConcurrentBackend, ProvisioningTerminator};
        use crate::{Error, Result};
        use async_trait::async_trait;
        use std::sync::{Arc, Mutex};
        use tokio::process::{Child, Command};

        /// Minimal D-Bus backend stub that is syntactically correct and implements the
        /// required traits. This is intentionally conservative: behaviour can be
        /// extended later to use zbus and query real wpa_supplicant properties.
        #[derive(Debug)]
        pub struct DbusBackend {
            hostapd_pid: Arc<Mutex<Option<u32>>>,
            dnsmasq: Arc<Mutex<Option<Child>>>,
        }

        impl DbusBackend {
            pub async fn new() -> Result<Self> {
                Ok(Self {
                    hostapd_pid: Arc::new(Mutex::new(None)),
                    dnsmasq: Arc::new(Mutex::new(None)),
                })
            }
        }

        #[async_trait]
        impl ConcurrentBackend for DbusBackend {
            async fn enter_provisioning_mode(&self) -> Result<()> {
                // Conservative stub: try to disconnect wpa_cli and start AP-related processes.
                let _ = Command::new("wpa_cli").arg("-i").arg("wlan0").arg("disconnect").output().await;

                // Try to start hostapd (best-effort)
                let maybe_child = Command::new("hostapd").arg("/etc/hostapd.conf").arg("-B").spawn();
                if let Ok(child) = maybe_child {
                    if let Some(pid) = child.id() {
                        *self.hostapd_pid.lock().unwrap() = Some(pid);
                    }
                }

                // dnsmasq best-effort (run in background)
                let maybe_dns = Command::new("dnsmasq").arg("--no-daemon").spawn();
                if let Ok(child) = maybe_dns {
                    *self.dnsmasq.lock().unwrap() = Some(child);
                }

                Ok(())
            }

            async fn scan(&self) -> Result<Vec<Network>> {
                // Minimal implementation: use wpa_cli scan_results if available, otherwise empty.
                let output = Command::new("wpa_cli").arg("-i").arg("wlan0").arg("scan_results").output().await;
                if let Ok(out) = output {
                    if out.status.success() {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        // Very small parser: lines with tab-separated fields
                        let mut networks = Vec::new();
                        for line in stdout.lines().skip(1) {
                            let parts: Vec<&str> = line.split('\t').collect();
                            if parts.len() >= 5 {
                                let ssid = parts[4].to_string();
                                if ssid.is_empty() || ssid == "\\x00" { continue; }
                                networks.push(Network { ssid, signal: 0, security: "Unknown".to_string() });
                            }
                        }
                        return Ok(networks);
                    }
                }
                Ok(Vec::new())
            }
        }

        #[async_trait]
        impl ProvisioningTerminator for DbusBackend {
            async fn is_connected(&self) -> Result<bool> {
                // Conservative default: assume not connected. A full implementation should
                // query wpa_supplicant via D-Bus and check the interface State.
                Ok(false)
            }

            async fn connect(&self, _ssid: &str, _password: &str) -> Result<()> {
                // Minimal stub: rely on existing wpa_supplicant tooling.
                Ok(())
            }

            async fn exit_provisioning_mode(&self) -> Result<()> {
                // Stop dnsmasq and hostapd if we started them.
                if let Some(mut child) = self.dnsmasq.lock().unwrap().take() {
                    let _ = child.kill().await;
                }
                if let Some(pid) = *self.hostapd_pid.lock().unwrap() {
                    let _ = Command::new("kill").arg(pid.to_string()).output().await;
                }
                Ok(())
            }
        }
            } else {
                tracing::warn!("IP address {} already exists on {}, proceeding.", AP_IP_ADDR, IFACE_NAME);
            }
        }

        println!("📡 [DbusBackend] Starting hostapd...");
        let child = Command::new("hostapd").arg("/etc/hostapd.conf").arg("-B").spawn()?;
        if let Some(pid) = child.id() {
            *self.hostapd_pid.lock().unwrap() = Some(pid);
        } else {
            return Err(Error::CommandFailed("Could not get PID for hostapd process".to_string()));
        }

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

    async fn scan(&self) -> Result<Vec<Network>> {
        println!("📡 [DbusBackend] Scanning for networks via wpa_cli...");

        let output = Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("scan").output().await?;
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            if error_msg.contains("Failed to connect to wpa_supplicant") {
                return Err(Error::CommandFailed("wpa_supplicant service is not running or not accessible".to_string()));
            }
            if error_msg.contains("rfkill") {
                return Err(Error::CommandFailed("Scan failed, device is blocked by rfkill".to_string()));
            }
            return Err(Error::CommandFailed(format!("wpa_cli scan failed: {}", error_msg)));
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let output = Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("scan_results").output().await?;
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(Error::CommandFailed(format!("wpa_cli scan_results failed: {}", error_msg)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_scan_results(&stdout)
    }
}

#[async_trait]
impl ProvisioningTerminator for DbusBackend {
    async fn is_connected(&self) -> Result<bool> {
        println!("📡 [DbusBackend] Checking connection status via D-Bus...");
        let iface_proxy = self.get_iface_proxy().await?;
        let prop_proxy = PropertiesProxy::new(&self.connection, WPA_S_SERVICE, iface_proxy.path()).await?;

        let props = prop_proxy.get_all("fi.w1.wpa_supplicant1.Interface").await?;
        if let Some(val) = props.get("State") {
            if let Ok(state_str) = <OwnedValue as TryInto<String>>::try_into(val.clone()) {
                if state_str == "completed" {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    async fn connect(&self, ssid: &str, password: &str) -> Result<()> {
        println!("📡 [DbusBackend] Attempting to connect to SSID: '{}' via D-Bus...", ssid);
        let iface_proxy = self.get_iface_proxy().await?;

        let mut args = HashMap::new();
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
        Ok(())
    }

    async fn exit_provisioning_mode(&self) -> Result<()> {
        println!("📡 [DbusBackend] Exiting provisioning mode...");

        let dnsmasq_child_to_kill = self.dnsmasq.lock().unwrap().take();
        if let Some(mut child) = dnsmasq_child_to_kill {
            let _ = child.kill().await;
        }

        let pid_to_kill = { *self.hostapd_pid.lock().unwrap() };
        if let Some(pid) = pid_to_kill {
            let _ = Command::new("kill").arg(pid.to_string()).output().await;
        }

        let _ = Command::new("ip").arg("addr").arg("del").arg(AP_IP_ADDR).arg("dev").arg(IFACE_NAME).output().await;

        let _ = Command::new("wpa_cli").arg("-i").arg(IFACE_NAME).arg("reconfigure").output().await;

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
    }

    #[async_trait]
    impl ProvisioningTerminator for DbusBackend {
        async fn is_connected(&self) -> Result<bool> {
            println!("📡 [DbusBackend] Checking connection status via D-Bus...");
            let prop_proxy = PropertiesProxy::new(&self.connection, WPA_S_SERVICE, WPA_S_PATH).await?;
            let iface_paths = WpaSupplicantProxy::new(&self.connection, WPA_S_SERVICE, WPA_S_PATH).await?.interfaces().await?;

            for path in iface_paths {
                let obj_path = ObjectPath::try_from(path.as_str())?;
                let props = PropertiesProxy::new(&self.connection, WPA_S_SERVICE, &obj_path).await?;
                let all = props.get_all("fi.w1.wpa_supplicant1.Interface").await?;
                if let Some(val) = all.get("State") {
                    if let Ok(state_str) = <OwnedValue as TryInto<String>>::try_into(val.clone()) {
                        if state_str.to_lowercase() == "completed" {
                            return Ok(true);
                        }
                    }
                }
            }
            Ok(false)
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
                "📡 [DbusBackend] Connection process initiated for '{}',",
                ssid
            );
            Ok(())
        }
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

