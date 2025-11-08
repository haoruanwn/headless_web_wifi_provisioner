// NetworkManager TDM backend (time-multiplexing)
// Minimal implementation using `nmcli` for scanning and `nmcli general` for state.
// This is intentionally conservative and best-effort; it mirrors the WpaCli TDM
// behaviour but uses NetworkManager where available.

use crate::traits::{ApConfig, ConnectionRequest, Network, PolicyCheck, TdmBackend};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::process::Command;
use std::net::{SocketAddr, Ipv4Addr};
use tokio::sync::Mutex;

const IFACE_NAME: &str = "wlan0";

#[derive(Debug)]
pub struct NetworkManagerTdmBackend {
    ap_config: Arc<ApConfig>,
    hotspot_name: Arc<Mutex<Option<String>>>,
    last_scan: Arc<Mutex<Option<Vec<Network>>>>,
}

impl NetworkManagerTdmBackend {
    pub fn new() -> Result<Self> {
        let cfg = ApConfig {
            ssid: "ProvisionerAP".to_string(),
            psk: "20542054".to_string(),
            bind_addr: SocketAddr::new(Ipv4Addr::new(192, 168, 4, 1).into(), 80),
            gateway_cidr: "192.168.4.1/24".to_string(),
        };
        Ok(Self {
            ap_config: Arc::new(cfg),
            hotspot_name: Arc::new(Mutex::new(None)),
            last_scan: Arc::new(Mutex::new(None)),
        })
    }

    /// 启动 AP（使用 `connection add` 以便指定 IP）
    async fn start_ap(&self) -> Result<()> {
        // 这个名称与 `stop_ap` 中要删除的名称一致
        const AP_CONNECTION_NAME: &str = "ProvisionerAP";

        // 1. 尝试添加一个新连接配置
        //    这与 AP配网模式.md 中的逻辑相同
        let add_output = Command::new("nmcli")
            .arg("connection")
            .arg("add")
            .arg("type")
            .arg("wifi")
            .arg("ifname")
            .arg(IFACE_NAME)
            .arg("con-name")
            .arg(AP_CONNECTION_NAME)
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
            // 如果连接已存在（例如上次程序崩溃未清理），也算成功
            if !err.contains("connection profile") || !err.contains("already exists") {
                return Err(Error::CommandFailed(format!(
                    "Failed to add hotspot connection: {}",
                    err
                )));
            }
        }

        // 2. 激活这个连接
        let up_output = Command::new("nmcli")
            .arg("connection")
            .arg("up")
            .arg(AP_CONNECTION_NAME)
            .output()
            .await?;

        if !up_output.status.success() {
            let err = String::from_utf8_lossy(&up_output.stderr);
            return Err(Error::CommandFailed(format!(
                "Failed to bring up hotspot connection: {}",
                err
            )));
        }

        // 3. 存储我们创建的连接名称，以便 stop_ap 可以清理它
        *self.hotspot_name.lock().await = Some(AP_CONNECTION_NAME.to_string());

        Ok(())
    }

    /// Stop the hotspot managed by NetworkManager (best-effort).
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
        // `nmcli -t -f SSID,SIGNAL,SECURITY device wifi list` yields colon-separated lines
        let mut networks = Vec::new();
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            // split into at most 3 fields
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
        // ask NetworkManager to rescan
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

    // Check whether NetworkManager reports a connected state.
    pub async fn check_connected_nmcli() -> Result<bool> {
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

    /// 轮询：检查 wlan0 是否已连接到 *特定* SSID
    async fn check_connected_to_ssid(ssid: &str) -> Result<bool> {
        let output = Command::new("nmcli")
            .arg("-t") // 简洁模式
            .arg("-f") // 字段
            .arg("NAME,DEVICE,STATE") // 获取 连接名, 设备, 状态
            .arg("connection")
            .arg("show")
            .arg("--active") // 只显示激活的连接
            .output()
            .await;

        match output {
            Ok(out) => {
                if !out.status.success() {
                    return Ok(false);
                }
                let stdout = String::from_utf8_lossy(&out.stdout);
                // 示例输出:
                // MyHomeWifi:wlan0:activated
                // eth0-conn:eth0:activated

                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() >= 3 {
                        let name = parts[0];   // e.g., "Xiaomi 14"
                        let device = parts[1]; // e.g., "wlan0"
                        let state = parts[2];  // e.g., "activated"

                        // 检查 激活的连接名 是否等于 目标SSID，
                        // 并且它是否在 wlan0 上，并且状态是 "activated"
                        if name == ssid && device == IFACE_NAME && state == "activated" {
                            return Ok(true); // 精确匹配成功
                        }
                    }
                }
                Ok(false) // 没有找到匹配的活动连接
            }
            Err(_) => Ok(false),
        }
    }
}

impl NetworkManagerTdmBackend {
    pub async fn enter_provisioning_mode_with_scan_impl(&self) -> Result<Vec<Network>> {
        // Ensure NetworkManager is running is out of scope; we rely on nmcli availability.
        let networks = self.scan_internal().await?;
        if networks.is_empty() {
            return Err(Error::CommandFailed(
                "Initial scan returned no networks".into(),
            ));
        }
        *self.last_scan.lock().await = Some(networks.clone());
        // start AP
        self.start_ap().await?;
        Ok(networks)
    }

    pub async fn connect_impl(&self, ssid: &str, password: &str) -> Result<()> {
        // 1. 停止 AP 模式
        self.stop_ap().await?;
        println!("📡 [NetworkManagerTDM] AP stopped.");

        // 2. 显式断开 wlan0 接口，清除可能的假阳性连接状态
        println!("📡 [NetworkManagerTDM] Disconnecting wlan0 from any existing network...");
        let _ = Command::new("nmcli")
            .arg("device")
            .arg("disconnect")
            .arg(IFACE_NAME)
            .status()
            .await;

        // 等待接口释放
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // -----------------------------------------------------------------
        // vvv [新修复] 强制执行一次新的扫描 vvv
        // -----------------------------------------------------------------
        println!("📡 [NetworkManagerTDM] Forcing device rescan...");
        let rescan_status = Command::new("nmcli")
            .arg("device")
            .arg("wifi")
            .arg("rescan") // <-- 命令 NM 重新扫描
            .status()      // <-- 等待 rescan 命令 *本身* 退出 (这很快)
            .await;
            
        if rescan_status.is_err() {
             println!("📡 [NetworkManagerTDM] 'nmcli rescan' command failed to start.");
        }
        
        // **关键**：给 NetworkManager 几秒钟时间来实际完成扫描并更新其内部缓存
        // (这个延迟是必要的，模拟了 wpa_cli_TDM 的 sleep)
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        println!("📡 [NetworkManagerTDM] Rescan complete (waited 5s).");
        // -----------------------------------------------------------------
        // ^^^ [新修复] ^^^
        // -----------------------------------------------------------------

        // 3. 异步 Spawn 连接命令
        println!("📡 [NetworkManagerTDM] Spawning connect command for '{}'...", ssid);
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

        // 检查 spawn 是否成功
        if let Err(e) = connect_cmd {
            println!("📡 [NetworkManagerTDM] Failed to spawn nmcli connect: {}", e);
            let _ = self.start_ap().await; // 恢复 AP
            return Err(Error::Io(e));
        }

        // 4. 使用新的、更精确的轮询函数检查是否连接到指定 SSID
        println!("📡 [NetworkManagerTDM] Polling for connection to '{}'...", ssid);
        for i in 0..20 {
            println!("📡 [NetworkManagerTDM] Polling... (Attempt {}/{})", i + 1, 20);
            if let Ok(true) = Self::check_connected_to_ssid(ssid).await {
                println!("📡 [NetworkManagerTDM] Connection to '{}' successful.", ssid);
                return Ok(());
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }

        // 5. 连接超时，恢复 AP 模式并返回错误
        println!("📡 [NetworkManagerTDM] Connection to '{}' timed out, restoring AP...", ssid);
        let _ = self.start_ap().await; // 恢复 AP

        Err(Error::CommandFailed(format!("Connection to '{}' timed out (20s)", ssid).into()))
    }

    async fn enter_provisioning_mode_impl(&self) -> Result<()> {
        // Similar to WpaCli: scan then start AP
        let networks = self.scan_internal().await?;
        if networks.is_empty() {
            return Err(Error::CommandFailed(
                "Initial scan returned no networks".into(),
            ));
        }
        *self.last_scan.lock().await = Some(networks);
        self.start_ap().await?;
        Ok(())
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
impl PolicyCheck for NetworkManagerTdmBackend {
    async fn is_connected(&self) -> Result<bool> {
        // Use `nmcli -t -f STATE general` which usually prints e.g. "connected" or "disconnected"
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
impl TdmBackend for NetworkManagerTdmBackend {
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