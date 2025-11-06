// 后端：wpa_cli_TDM（时分复用调用 wpa_cli）
// 基于之前的 wpa_cli_exclusive2 实现做了重命名并修复了 dnsmasq --address 参数。

use crate::traits::{Network, ProvisioningBackend, TdmBackend};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

const IFACE_NAME: &str = "wlan0";
const AP_IP_ADDR: &str = "192.168.4.1/24";

#[derive(Debug)]
pub struct WpaCliTdmBackend {
    // 控制 hostapd 进程的句柄
    hostapd: Arc<Mutex<Option<Child>>> ,
    dnsmasq: Arc<Mutex<Option<Child>>> ,
    // 上一次扫描结果（应用启动时会先执行一次扫描并保存）
    last_scan: Arc<Mutex<Option<Vec<Network>>>>,
}

impl WpaCliTdmBackend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            hostapd: Arc::new(Mutex::new(None)),
            dnsmasq: Arc::new(Mutex::new(None)),
            last_scan: Arc::new(Mutex::new(None)),
        })
    }

    // 解析 wpa_cli scan_results
    fn parse_scan_results(output: &str) -> Result<Vec<Network>> {
        let mut networks = Vec::new();
        for line in output.lines().skip(1) {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                let signal_level: i16 = parts[2].parse().unwrap_or(0);
                let flags = parts[3];
                let ssid = parts[4].to_string();

                if ssid.is_empty() || ssid == "\x00" {
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

    /// 启动 AP（仅启动 hostapd/dnsmasq 并设置 IP），不做扫描
    async fn start_ap(&self) -> Result<()> {
        // 在启动 AP 之前，清理可能残留的进程（hostapd/dnsmasq/wpa_supplicant）
        let _ = Command::new("killall")
            .arg("-9")
            .arg("hostapd")
            .arg("dnsmasq")
            .arg("wpa_supplicant")
            .status()
            .await;

        // 设置 IP
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

        // 启动 hostapd
        let child = Command::new("hostapd")
            .arg("/etc/hostapd.conf")
            .arg("-B")
            .spawn()?;
        *self.hostapd.lock().await = Some(child);

        // 启动 dnsmasq
        let ap_ip_only = AP_IP_ADDR.split('/').next().unwrap_or("");
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

    /// 停止 AP（停止 hostapd/dnsmasq 并移除 IP），并尝试启动 wpa_supplicant
    async fn stop_ap(&self) -> Result<()> {
        if let Some(mut child) = self.dnsmasq.lock().await.take() {
            let _ = child.kill().await;
        }
        if let Some(mut child) = self.hostapd.lock().await.take() {
            let _ = child.kill().await;
        }

        // cleanup IP
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

        // 尝试启动 wpa_supplicant（先清理可能残留的 wpa_supplicant）
        let _ = Command::new("killall")
            .arg("-9")
            .arg("wpa_supplicant")
            .status()
            .await;
        let _ = Command::new("wpa_supplicant")
            .arg("-B")
            .arg(format!("-i{}", IFACE_NAME))
            .arg("-c/etc/wpa_supplicant.conf")
            .spawn()?;

        Ok(())
    }

    /// 执行一次真实的 wpa_cli 扫描并返回结果（不启动/停止 AP）
    async fn scan_internal(&self) -> Result<Vec<Network>> {
        // 触发扫描
        let output = Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("scan")
            .output()
            .await?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(Error::CommandFailed(format!(
                "wpa_cli scan failed: {}",
                error_msg
            )));
        }

        // 等待一会儿以获取结果
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

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
        // debug 输出
        println!("📡 [WpaCliTDM] --- SCAN RESULTS ---");
        println!("{}", stdout);
        println!("📡 [WpaCliTDM] --------------------");

        let networks = Self::parse_scan_results(&stdout)?;
        Ok(networks)
    }
}

#[async_trait]
impl ProvisioningBackend for WpaCliTdmBackend {
    /// 应用启动时会调用此方法（主程序会调用一次）。
    /// 我们的策略：先确保处于 STA 并扫描一次。
    /// - 如果扫描为空 -> 返回错误，停止后续操作。
    /// - 如果扫描有结果 -> 保存结果并启动 AP（展示结果）。
    async fn enter_provisioning_mode(&self) -> Result<()> {
        println!("📡 [WpaCliTDM] Initializing: entering STA to scan...");

        // 确保 wpa_supplicant 运行
        let _ = Command::new("wpa_supplicant")
            .arg("-B")
            .arg(format!("-i{}", IFACE_NAME))
            .arg("-c/etc/wpa_supplicant.conf")
            .spawn();

        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        // 进行一次扫描
        let networks = self.scan_internal().await?;

        if networks.is_empty() {
            println!("📡 [WpaCliTDM] Initial scan returned no networks. Aborting startup.");
            return Err(Error::CommandFailed("Initial scan returned no networks".into()));
        }

        // 存储结果
        *self.last_scan.lock().await = Some(networks);

        // 切换为 AP，展示结果
        println!("📡 [WpaCliTDM] Initial scan found networks, starting AP to serve UI...");
        self.start_ap().await?;

        Ok(())
    }

    async fn exit_provisioning_mode(&self) -> Result<()> {
        println!("📡 [WpaCliTDM] Exiting provisioning mode (stop AP)");
        self.stop_ap().await?;
        Ok(())
    }

    /// 返回保存在本地的扫描结果（如果存在），否则执行实时扫描
    async fn scan(&self) -> Result<Vec<Network>> {
        if let Some(vec) = &*self.last_scan.lock().await {
            return Ok(vec.clone());
        }
        let networks = self.scan_internal().await?;
        *self.last_scan.lock().await = Some(networks.clone());
        Ok(networks)
    }

    /// 连接逻辑：切换到 STA 尝试连接；失败后重新扫描并恢复 AP，并返回错误信息（会在 Web 界面展示）
    async fn connect(&self, ssid: &str, password: &str) -> Result<()> {
        println!("📡 [WpaCliTDM] Attempting connect: switching to STA...");

        // 停止 AP 并确保 wpa_supplicant 运行
        self.stop_ap().await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // add_network
        let output = Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("add_network")
            .output()
            .await?;
        if !output.status.success() {
            return Err(Error::CommandFailed("wpa_cli add_network failed".to_string()));
        }
        let network_id_str = String::from_utf8(output.stdout).map_err(|e| Error::CommandFailed(format!("Failed to parse wpa_cli output: {}", e)))?;
        let network_id: u32 = match network_id_str.trim().parse::<u32>() {
            Ok(n) => n,
            Err(_) => {
                return Err(Error::CommandFailed(format!(
                    "Failed to parse network ID from wpa_cli: {}",
                    network_id_str
                )));
            }
        };

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

        // 检查连接状态
        println!("📡 [WpaCliTDM] Waiting for connection result...");
        for _ in 0..30 {
            let status_output = Command::new("wpa_cli")
                .arg("-i")
                .arg(IFACE_NAME)
                .arg("status")
                .output()
                .await?;

            if !status_output.status.success() {
                return Err(Error::CommandFailed("Failed to get wpa_cli status".into()));
            }
            let status_str = String::from_utf8_lossy(&status_output.stdout);
            if status_str.contains("wpa_state=COMPLETED") {
                println!("📡 [WpaCliTDM] Connection successful (COMPLETED). Saving config...");
                Command::new("wpa_cli")
                    .arg("-i")
                    .arg(IFACE_NAME)
                    .arg("save_config")
                    .status()
                    .await?;
                // 成功后自动获取 DHCP（在后台运行 udhcpc），避免手动运行 `udhcpc -i wlan0`
                let _ = Command::new("udhcpc")
                    .arg("-i")
                    .arg(IFACE_NAME)
                    .spawn();
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                return Ok(());
            }
            if status_str.contains("reason=WRONG_KEY") {
                println!("📡 [WpaCliTDM] Connection failed: WRONG_KEY");
                Command::new("wpa_cli")
                    .arg("-i")
                    .arg(IFACE_NAME)
                    .arg("remove_network")
                    .arg(network_id.to_string())
                    .status()
                    .await?;

                // 连接失败后重新扫描并恢复 AP，向前端展示错误
                let networks = self.scan_internal().await.unwrap_or_default();
                *self.last_scan.lock().await = Some(networks);
                let _ = self.start_ap().await;

                return Err(Error::CommandFailed("Invalid password".into()));
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }

        // 超时
        println!("📡 [WpaCliTDM] Connection timed out");
        let _ = Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("remove_network")
            .arg(network_id.to_string())
            .status()
            .await;

        // 重新扫描并恢复 AP
        let networks = self.scan_internal().await.unwrap_or_default();
        *self.last_scan.lock().await = Some(networks);
        let _ = self.start_ap().await;

        Err(Error::CommandFailed("Connection timed out".into()))
    }
}

#[async_trait]
impl TdmBackend for WpaCliTdmBackend {
    async fn enter_provisioning_mode_with_scan(&self) -> Result<Vec<Network>> {
        // reuse existing initialization that performs an initial scan and starts AP
        ProvisioningBackend::enter_provisioning_mode(self).await?;
        if let Some(vec) = &*self.last_scan.lock().await {
            Ok(vec.clone())
        } else {
            Err(Error::CommandFailed("Initial scan yielded no networks".into()))
        }
    }
}
