// 文件: provisioner-core/src/backends/wpa_cli_exclusive/mod.rs
use crate::traits::{Network, ProvisioningBackend};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, error, info, trace, warn};
use std::process::Output;

const IFACE_NAME: &str = "wlan0";
const AP_IP_ADDR: &str = "192.168.4.1/24";

/// 一个基于分时复用的后端，使用 hostapd, dnsmasq 和 wpa_cli。
/// 适用于不支持并发的硬件。
#[derive(Debug)]
pub struct WpaCliExclusiveBackend {
    // 复用 DbusBackend 的进程管理
    hostapd: Arc<Mutex<Option<Child>>>,
    dnsmasq: Arc<Mutex<Option<Child>>>,
}

impl WpaCliExclusiveBackend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            hostapd: Arc::new(Mutex::new(None)),
            dnsmasq: Arc::new(Mutex::new(None)),
        })
    }

    // 帮助函数：解析 wpa_cli scan_results
    // (逻辑完全复制自 WpaCliDnsmasqBackend::parse_scan_results)
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
}

// Helper: run a command and return Output; produce a unified Error::CommandFailed on failure
async fn run_cmd_output(mut cmd: Command, ctx: &str) -> Result<Output> {
    match cmd.output().await {
        Ok(out) => {
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                error!(context = %ctx, stderr = %err, "command failed");
                return Err(Error::CommandFailed(format!("{} failed: {}", ctx, err)));
            }
            Ok(out)
        }
        Err(e) => {
            error!(context = %ctx, error = %e, "failed to spawn command");
            Err(Error::CommandFailed(format!("{} spawn failed: {}", ctx, e)))
        }
    }
}

// Helper: run a command expecting a status success, no output returned
async fn run_cmd_status(mut cmd: Command, ctx: &str) -> Result<()> {
    match cmd.status().await {
        Ok(status) => {
            if !status.success() {
                return Err(Error::CommandFailed(format!("{} returned non-zero", ctx)));
            }
            Ok(())
        }
        Err(e) => Err(Error::CommandFailed(format!("{} spawn failed: {}", ctx, e))),
    }
}

#[async_trait]
impl ProvisioningBackend for WpaCliExclusiveBackend {

    /// 启动 AP 模式
    /// (逻辑复用自 DbusBackend::enter_provisioning_mode)
    async fn enter_provisioning_mode(&self) -> Result<()> {
        println!("📡 [WpaCliExclusive] Entering provisioning mode...");
        
        // 1. 确保 wpa_supplicant 已停止
        let _ = Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("terminate")
            .output()
            .await;
        
        // 2. 设置 IP
        // (逻辑复用自)
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

        // 3. 启动 hostapd
        // (逻辑复用自)
        let child = Command::new("hostapd")
            .arg("/etc/hostapd.conf") // 确保这个文件存在
            .arg("-B")
            .spawn()?;
        *self.hostapd.lock().await = Some(child);

        // 4. 启动 dnsmasq
        // (逻辑复用自)
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

    /// 停止 AP 模式
    /// (逻辑复用自 DbusBackend::exit_provisioning_mode)
    async fn exit_provisioning_mode(&self) -> Result<()> {
        println!("📡 [WpaCliExclusive] Exiting provisioning mode...");
        
        // 1. 停止 dnsmasq
        if let Some(mut child) = self.dnsmasq.lock().await.take() {
            let _ = child.kill().await;
        }

        // 2. 停止 hostapd
        if let Some(mut child) = self.hostapd.lock().await.take() {
            let _ = child.kill().await;
        }

        // 3. 清理 IP
        // (逻辑复用自)
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

        // 4. 启动 wpa_supplicant (为 STA 模式准备)
        let _ = Command::new("wpa_supplicant")
            .arg("-B")
            .arg(format!("-i{}", IFACE_NAME))
            .arg("-c/etc/wpa_supplicant.conf") // 确保这个文件存在
            .spawn()?;

        println!("📡 [WpaCliExclusive] Provisioning mode exited.");
        Ok(())
    }

    /// 扫描 (分时复用)
    async fn scan(&self) -> Result<Vec<Network>> {
        println!("📡 [WpaCliExclusive] Stopping AP mode for scanning...");
        // 1. 停止 AP
        self.exit_provisioning_mode().await?;
        
        // 等待 wpa_supplicant 启动
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        println!("📡 [WpaCliExclusive] Scanning via wpa_cli...");
        // 2. 执行扫描
        // (逻辑复用自 WpaCliDnsmasqBackend::scan)
        let output = Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("scan")
            .output()
            .await?;

        if !output.status.success() {
            // (错误处理)
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(Error::CommandFailed(format!(
                "wpa_cli scan failed: {}",
                error_msg
            )));
        }

        // 等待更长的时间以降低时序（race）问题的概率
        println!("📡 [WpaCliExclusive] Waiting for scan results (5 seconds)...");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        let output = Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("scan_results")
            .output()
            .await?;

        if !output.status.success() {
            // (错误处理)
             let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(Error::CommandFailed(format!(
                "wpa_cli scan_results failed: {}",
                error_msg
            )));
        }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // 关键调试日志：输出 scan_results 原始文本，便于排查空结果的原因
    println!("📡 [WpaCliExclusive] --- SCAN RESULTS ---");
    println!("{}", stdout);
    println!("📡 [WpaCliExclusive] --------------------");
        let networks = Self::parse_scan_results(&stdout)?;

        // 3. 重启 AP
        println!("📡 [WpaCliExclusive] Scan complete. Restarting AP mode...");
        self.enter_provisioning_mode().await?;

        // 4. 返回结果
        Ok(networks)
    }

    /// 连接 (终止操作)
    async fn connect(&self, ssid: &str, password: &str) -> Result<()> {
        println!("📡 [WpaCliExclusive] Stopping AP mode permanently...");
        // 1. 停止 AP
        self.exit_provisioning_mode().await?;
        
        // 等待 wpa_supplicant 准备就绪
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        println!("📡 [WpaCliExclusive] Attempting connect via wpa_cli...");
        // 2. 执行连接
        // (逻辑完全复制自 WpaCliDnsmasqBackend::connect)
        
        // 
        let output = Command::new("wpa_cli")
            .arg("-i")
            .arg(IFACE_NAME)
            .arg("add_network")
            .output()
            .await?;
        if !output.status.success() {
            return Err(Error::CommandFailed(
                "wpa_cli add_network failed".to_string(),
            ));
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

        // 
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

        // 3. 轮询连接状态
        // (逻辑复用自)
        println!("📡 [WpaCliExclusive] Waiting for connection result...");
        for _ in 0..30 { // Max wait 30 seconds
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
                println!("📡 [WpaCliExclusive] Connection successful (COMPLETED).");
                Command::new("wpa_cli")
                    .arg("-i")
                    .arg(IFACE_NAME)
                    .arg("save_config")
                    .status()
                    .await?;
                return Ok(());
            }
            
            if status_str.contains("reason=WRONG_KEY") {
                 println!("📡 [WpaCliExclusive] Connection failed: WRONG_KEY");
                 Command::new("wpa_cli")
                    .arg("-i")
                    .arg(IFACE_NAME)
                    .arg("remove_network")
                    .arg(network_id.to_string())
                    .status().await?;
                 return Err(Error::CommandFailed("Invalid password".into()));
            }
    
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    
        Err(Error::CommandFailed("Connection timed out".into()))
    }
}