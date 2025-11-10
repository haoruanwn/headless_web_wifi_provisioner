use crate::config::{ApConfig, ap_config_from_toml_str};
use crate::structs::{ConnectionRequest, Network};
use anyhow::{Result, anyhow, Context};
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::fs;
use tokio::process::Command;
use tokio::sync::broadcast;
use wpa_ctrl::{WpaController, WpaControllerBuilder};

// 从配置文件加载 AP 配置
static GLOBAL_AP_CONFIG: Lazy<ApConfig> = Lazy::new(|| {
    const CONFIG_TOML: &str = include_str!("../config/wpa_dbus.toml");
    ap_config_from_toml_str(CONFIG_TOML)
});

/// wpa_supplicant 控制套接字后端实现
pub struct WpaCtrlBackend {
    ap_config: Arc<ApConfig>,
    hostapd: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    dnsmasq: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    cmd_ctrl: Arc<Mutex<Option<WpaController>>>,
    event_tx: broadcast::Sender<String>,
}

impl WpaCtrlBackend {
    pub fn new() -> Result<Self> {
        let config = GLOBAL_AP_CONFIG.clone();

        // 创建 wpa_supplicant 配置文件，使用控制套接字接口
        let update_config_str = if config.wpa_update_config { "1" } else { "0" };
        let wpa_conf_content = format!(
            "ctrl_interface=DIR={} GROUP={}\nupdate_config={}\n",
            config.wpa_ctrl_interface, config.wpa_group, update_config_str
        );
        std::fs::write(&config.wpa_conf_path, wpa_conf_content.as_bytes())
            .context("Failed to write wpa_supplicant config")?;

        tracing::info!("Created wpa_supplicant config at: {}", config.wpa_conf_path);

        // 确保 wpa_supplicant 守护进程在运行
        Self::ensure_wpa_supplicant_daemon(&config)?;

        // 建立用于命令的 WpaController 连接
        tracing::debug!("Connecting CMD controller to {}", config.interface_name);
        
        let cmd_ctrl = WpaControllerBuilder::new()
            .open(&config.interface_name)
            .context("Failed to connect WpaController socket. Is wpa_supplicant running?")?;

        let cmd_ctrl_arc = Arc::new(Mutex::new(Some(cmd_ctrl)));

        // 创建一个广播通道用于事件
        let (event_tx, _) = broadcast::channel(32);

        // 启动一个专用线程来监听事件
        Self::start_event_listener_thread(&config, event_tx.clone())?;

        Ok(Self {
            ap_config: Arc::new(config),
            hostapd: Arc::new(tokio::sync::Mutex::new(None)),
            dnsmasq: Arc::new(tokio::sync::Mutex::new(None)),
            cmd_ctrl: cmd_ctrl_arc,
            event_tx,
        })
    }

    pub fn ap_config(&self) -> Arc<ApConfig> {
        self.ap_config.clone()
    }

    /// 辅助函数：确保 wpa_supplicant 在运行
    fn ensure_wpa_supplicant_daemon(config: &ApConfig) -> Result<()> {
        // 尝试连接一下，看是否已在运行
        if WpaControllerBuilder::new()
            .open(&config.interface_name)
            .is_ok()
        {
            tracing::debug!("wpa_supplicant already running.");
            return Ok(());
        }

        tracing::info!("wpa_supplicant not running, attempting to start...");

        // 清理残留进程
        let _ = std::process::Command::new("killall")
            .arg("wpa_supplicant")
            .status();

        // 启动 wpa_supplicant 守护进程
        let status = std::process::Command::new("wpa_supplicant")
            .arg("-B")
            .arg(format!("-i{}", config.interface_name))
            .arg("-c")
            .arg(&config.wpa_conf_path)
            .status()
            .context("Failed to spawn wpa_supplicant daemon")?;

        if !status.success() {
            return Err(anyhow!("wpa_supplicant daemon failed to start"));
        }

        // 等待 socket 文件出现
        std::thread::sleep(Duration::from_secs(2));
        Ok(())
    }

    /// 辅助函数：在专用线程中监听 wpa_supplicant 事件
    fn start_event_listener_thread(config: &ApConfig, tx: broadcast::Sender<String>) -> Result<()> {
        let iface_name = config.interface_name.clone();

        std::thread::spawn(move || {
            tracing::debug!("Event listener thread started.");

            // 无限重连循环，确保鲁棒性
            loop {
                let mut event_ctrl = match WpaControllerBuilder::new()
                    .open(&iface_name)
                {
                    Ok(ctrl) => ctrl,
                    Err(e) => {
                        tracing::error!("Event controller connect failed, retrying in 5s: {}", e);
                        std::thread::sleep(Duration::from_secs(5));
                        continue;
                    }
                };

                // 对于事件监听，我们需要持续接收消息
                // wpa-ctrl 不需要显式的 "attach"，它会发送事件

                tracing::info!("Event listener connected to wpa_supplicant.");

                // 阻塞循环，接收事件
                let mut recv_err_count = 0;
                loop {
                    match event_ctrl.recv() {
                        Ok(Some(msg)) => {
                            let msg_str = msg.raw.to_string();
                            recv_err_count = 0;
                            tracing::trace!(event = %msg_str, "Received wpa_event");
                            if tx.send(msg_str).is_err() {
                                tracing::warn!("No listeners for wpa_event");
                            }
                        }
                        Ok(None) => {
                            tracing::trace!("Event listener received None");
                            recv_err_count += 1;
                            if recv_err_count > 3 {
                                tracing::warn!("Too many empty receives. Reconnecting...");
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        Err(e) => {
                            tracing::warn!("Event listener recv error: {}. Reconnecting...", e);
                            break;
                        }
                    }
                }

                tracing::warn!("Event listener detached. wpa_supplicant may have exited. Re-attaching in 500ms...");
                std::thread::sleep(Duration::from_millis(500));
            }
        });

        Ok(())
    }

    /// 内部函数：发送一个命令并获取回复
    /// 这是阻塞 I/O，所以必须在 spawn_blocking 中运行
    async fn send_cmd(&self, cmd: String) -> Result<String> {
        let ctrl_clone = self.cmd_ctrl.clone();
        tokio::task::spawn_blocking(move || {
            let mut ctrl_opt = ctrl_clone.lock().unwrap();
            if let Some(ref mut ctrl) = *ctrl_opt {
                // 使用 WpaControlReq 来发送原始命令
                use wpa_ctrl::WpaControlReq;
                
                ctrl.request(WpaControlReq::raw(&cmd))
                    .map_err(|e| anyhow!("wpa_ctrl request failed: {}", e))?;
                
                // 然后接收回复
                match ctrl.recv() {
                    Ok(Some(msg)) => Ok(msg.raw.to_string()),
                    Ok(None) => Err(anyhow!("No response received")),
                    Err(e) => Err(anyhow!("recv failed: {}", e)),
                }
            } else {
                Err(anyhow!("WpaController not available"))
            }
        })
        .await
        .context("spawn_blocking task failed")?
    }

    /// 辅助函数：解析 SCAN_RESULTS 的输出
    /// 格式: bssid / frequency / signal level / flags / ssid
    fn parse_scan_results(output: &str) -> Result<Vec<Network>> {
        let mut networks = Vec::new();
        for line in output.lines().skip(1) {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 5 {
                continue;
            }

            let signal_dbm: i16 = parts[2].parse().unwrap_or(-100);
            let flags = parts[3];
            let ssid = parts[4].to_string();

            if ssid.is_empty() {
                continue;
            }

            let security = if flags.contains("WPA2") {
                "WPA2".to_string()
            } else if flags.contains("WPA") {
                "WPA".to_string()
            } else {
                "Open".to_string()
            };

            let signal_percent = ((signal_dbm.clamp(-100, -50) + 100) * 2) as u8;

            networks.push(Network {
                ssid,
                signal: signal_percent,
                security,
            });
        }
        Ok(networks)
    }

    /// 内部扫描方法
    async fn scan_internal(&self) -> Result<Vec<Network>> {
        // 订阅事件
        let mut rx = self.event_tx.subscribe();

        tracing::debug!("Sending SCAN command...");
        self.send_cmd("SCAN".to_string()).await?;

        // 异步等待扫描结果事件
        let fut = async {
            loop {
                match rx.recv().await {
                    Ok(event) if event.contains("CTRL-EVENT-SCAN-RESULTS") => {
                        return Ok::<(), anyhow::Error>(());
                    }
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(e) => return Err(anyhow!("recv error: {}", e)),
                }
            }
        };

        // 超时
        match tokio::time::timeout(Duration::from_secs(15), fut).await {
            Ok(Ok(_)) => tracing::debug!("Scan results event received."),
            Ok(Err(e)) => return Err(anyhow!("Failed waiting for scan event: {}", e)),
            Err(_) => return Err(anyhow!("Scan timed out")),
        }

        // 获取结果并解析
        let results_str = self.send_cmd("SCAN_RESULTS".to_string()).await?;
        Self::parse_scan_results(&results_str)
    }

    /// 启动 AP 模式
    async fn start_ap(&self) -> Result<()> {
        // 使用 stop_ap() 而不是粗暴的 killall
        let _ = self.stop_ap().await;

        // 配置 IP 地址
        let output = Command::new("ip")
            .arg("addr")
            .arg("add")
            .arg(&self.ap_config.gateway_cidr)
            .arg("dev")
            .arg(&self.ap_config.interface_name)
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            if !err.contains("File exists") {
                return Err(anyhow!("Failed to set IP: {}", err));
            }
        }

        // 生成 hostapd 配置
        let hostapd_conf = format!(
            "interface={}\nssid={}\nwpa={}\nwpa_passphrase={}\nhw_mode={}\nchannel={}\nwpa_key_mgmt={}\nwpa_pairwise={}\nrsn_pairwise={}\n",
            self.ap_config.interface_name,
            self.ap_config.ssid,
            self.ap_config.hostapd_wpa,
            self.ap_config.psk,
            self.ap_config.hostapd_hw_mode,
            self.ap_config.hostapd_channel,
            self.ap_config.hostapd_wpa_key_mgmt,
            self.ap_config.hostapd_wpa_pairwise,
            self.ap_config.hostapd_rsn_pairwise
        );

        // 写入 hostapd 配置文件
        fs::write(&self.ap_config.hostapd_conf_path, hostapd_conf.as_bytes()).await?;
        tracing::debug!(
            "Created hostapd config at: {}",
            self.ap_config.hostapd_conf_path
        );

        // 启动 hostapd
        let child = Command::new("hostapd")
            .arg(&self.ap_config.hostapd_conf_path)
            .arg("-B")
            .spawn()?;
        *self.hostapd.lock().await = Some(child);

        // 启动 dnsmasq
        let ap_ip_only = self.ap_config.gateway_cidr.split('/').next().unwrap_or("");
        let dnsmasq_child = Command::new("dnsmasq")
            .arg(format!("--interface={}", self.ap_config.interface_name))
            .arg(format!("--dhcp-range={}", self.ap_config.dhcp_range))
            .arg(format!("--address=/#/{}", ap_ip_only))
            .arg("--no-resolv")
            .arg("--no-hosts")
            .arg("--no-daemon")
            .spawn()?;

        *self.dnsmasq.lock().await = Some(dnsmasq_child);
        tracing::info!(
            "AP started successfully on {}",
            self.ap_config.interface_name
        );
        Ok(())
    }

    /// 停止 AP 模式
    async fn stop_ap(&self) -> Result<()> {
        // 杀死我们启动的进程
        if let Some(mut child) = self.dnsmasq.lock().await.take() {
            let _ = child.kill().await;
        }
        if let Some(mut child) = self.hostapd.lock().await.take() {
            let _ = child.kill().await;
        }

        // 移除 IP 地址配置
        let output = Command::new("ip")
            .arg("addr")
            .arg("del")
            .arg(&self.ap_config.gateway_cidr)
            .arg("dev")
            .arg(&self.ap_config.interface_name)
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            if !err.contains("Cannot assign requested address") {
                return Err(anyhow!("Failed to clean IP: {}", err));
            }
        }

        // 清理 hostapd 配置文件
        let _ = fs::remove_file(&self.ap_config.hostapd_conf_path).await;

        tracing::info!("AP stopped on {}", self.ap_config.interface_name);
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

        // 订阅事件
        let mut rx = self.event_tx.subscribe();

        tracing::debug!("Adding new network...");
        let net_id_str = self.send_cmd("ADD_NETWORK".to_string()).await?;
        let net_id = net_id_str.trim().parse::<u32>()
            .context("Failed to parse ADD_NETWORK response")?;

        tracing::debug!(net_id, "Configuring network...");

        // 设置 SSID
        self.send_cmd(format!("SET_NETWORK {} ssid \"{}\"", net_id, req.ssid)).await?;

        // 设置密码或开放网络
        if req.password.is_empty() {
            self.send_cmd(format!("SET_NETWORK {} key_mgmt NONE", net_id)).await?;
        } else {
            self.send_cmd(format!("SET_NETWORK {} psk \"{}\"", net_id, req.password)).await?;
        }

        // 启用网络
        self.send_cmd(format!("ENABLE_NETWORK {}", net_id)).await?;

        // 异步等待连接成功事件
        let fut = async {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if event.contains("CTRL-EVENT-CONNECTED") {
                            return Ok::<(), anyhow::Error>(());
                        }
                        if event.contains("CTRL-EVENT-NETWORK-NOT-FOUND")
                            || event.contains("CTRL-EVENT-AUTH-REJECT")
                        {
                            return Err(anyhow!("Connection failed: {}", event));
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(e) => return Err(anyhow!("recv error: {}", e)),
                }
            }
        };

        // 超时
        match tokio::time::timeout(Duration::from_secs(30), fut).await {
            Ok(Ok(_)) => {
                tracing::info!("Connection successful!");
                // 成功后，可以选择保存配置
                if self.ap_config.wpa_update_config {
                    let _ = self.send_cmd("SAVE_CONFIG".to_string()).await;
                }
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                // 超时：清理网络并尝试恢复 AP
                let _ = self.send_cmd(format!("REMOVE_NETWORK {}", net_id)).await;
                let _ = self.start_ap().await;
                Err(anyhow!("Connection timed out"))
            }
        }
    }
}
