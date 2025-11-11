use crate::config::{ApConfig, ap_config_from_toml_str};
use crate::structs::{ConnectionRequest, Network};
use anyhow::{Result, anyhow, Context};
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::fs;
use tokio::process::Command;
use wpa_ctrl::{WpaController, WpaControllerBuilder};

// 从配置文件加载 AP 配置
static GLOBAL_AP_CONFIG: Lazy<ApConfig> = Lazy::new(|| {
    const CONFIG_TOML: &str = include_str!("../config/wpa_dbus.toml");
    ap_config_from_toml_str(CONFIG_TOML)
});

/// wpa_supplicant 控制套接字后端实现（轮询模式）
pub struct WpaCtrlBackend {
    ap_config: Arc<ApConfig>,
    hostapd: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    dnsmasq: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    cmd_ctrl: Arc<Mutex<Option<WpaController>>>,
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
        // 清理可能存在的陈旧客户端套接字文件，避免 "Address in use" 错误
        // wpa_ctrl 库默认在 /tmp 中创建客户端套接字
        Self::cleanup_stale_wpa_ctrl_sockets()?;

        tracing::debug!("Connecting CMD controller to {}", config.interface_name);
        
        let cmd_ctrl = WpaControllerBuilder::new()
            .open(&config.interface_name)
            .context("Failed to connect WpaController socket. Is wpa_supplicant running?")?;

        let cmd_ctrl_arc = Arc::new(Mutex::new(Some(cmd_ctrl)));

        Ok(Self {
            ap_config: Arc::new(config),
            hostapd: Arc::new(tokio::sync::Mutex::new(None)),
            dnsmasq: Arc::new(tokio::sync::Mutex::new(None)),
            cmd_ctrl: cmd_ctrl_arc,
        })
    }

    pub fn ap_config(&self) -> Arc<ApConfig> {
        self.ap_config.clone()
    }

    /// 辅助函数：清理陈旧的 wpa_ctrl 客户端套接字文件
    /// wpa_ctrl 库在 /tmp 中创建客户端套接字，如果程序之前崩溃，
    /// 这些文件可能会残留下来，导致 "Address in use" 错误
    fn cleanup_stale_wpa_ctrl_sockets() -> Result<()> {
        let tmp_path = std::path::Path::new("/tmp");
        
        // 尝试清理常见的 wpa_ctrl 套接字文件模式
        // 注意：只清理特定的套接字前缀，避免误删其他文件（如配置文件）
        let socket_patterns = vec![
            "wpa_ctrl_",           // wpa_ctrl 库的默认前缀
            "provisioner_ctrl_",    // 我们创建的唯一套接字前缀
        ];
        
        if let Ok(entries) = std::fs::read_dir(tmp_path) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let name_str = file_name.to_string_lossy();
                let path = entry.path();
                
                // 检查是否匹配我们要清理的模式，并且必须是文件而不是目录
                if socket_patterns.iter().any(|pattern| name_str.starts_with(pattern)) && path.is_file() {
                    match std::fs::remove_file(&path) {
                        Ok(_) => tracing::debug!("Removed stale socket: {:?}", path),
                        Err(e) => tracing::warn!("Failed to remove {:?}: {}", path, e),
                    }
                }
            }
        }
        
        Ok(())
    }

    /// 辅助函数：确保 wpa_supplicant 在运行
    fn ensure_wpa_supplicant_daemon(config: &ApConfig) -> Result<()> {
        let socket_path = std::path::Path::new(&config.wpa_ctrl_interface)
            .join(&config.interface_name);

        if socket_path.exists() {
            tracing::debug!("wpa_supplicant socket found at {:?}. Assuming it's running.", socket_path);
            return Ok(());
        }

        tracing::info!("wpa_supplicant socket not found, attempting to start...");

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
        tracing::info!("wpa_supplicant daemon started. Waiting for socket file...");
        std::thread::sleep(Duration::from_secs(2));
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

    /// 内部扫描方法（轮询模式）
    async fn scan_internal(&self) -> Result<Vec<Network>> {
        tracing::debug!("Sending SCAN command...");
        self.send_cmd("SCAN".to_string()).await?;

        // 固定等待 10 秒以确保扫描完成
        tracing::debug!("Waiting 10 seconds for scan results...");
        tokio::time::sleep(Duration::from_secs(10)).await;
        
        tracing::debug!("Scan wait complete, fetching results.");
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
        let mut networks;
        let max_retries = 3;
        let mut retry_count = 0;

        loop {
            // 1. 尝试执行内部扫描
            // (scan_internal 内部已经包含了 10 秒的等待)
            println!("Attempting to scan for networks (attempt {}/{})...", retry_count + 1, max_retries);
            networks = self.scan_internal().await?;
            
            // 2. 检查结果
            if networks.is_empty() {
                retry_count += 1;
                // 3. 如果为空，检查是否达到最大重试次数
                if retry_count >= max_retries {
                    tracing::error!(
                        "Scan failed after {} attempts. Check dmesg for driver/firmware errors. Cleaning up and exiting.",
                        max_retries
                    );
                    // 清理 AP 资源并退出
                    let _ = self.stop_ap().await;
                    return Err(anyhow!(
                        "Failed to scan for networks after {} attempts",
                        max_retries
                    ));
                }
                
                // 尚未达到最大重试次数，等待后重试
                tracing::warn!(
                    "Scan returned no networks. Retrying in 10 seconds... (attempt {}/{}, Check dmesg for driver/firmware errors)",
                    retry_count,
                    max_retries
                );
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue; // 回到 loop 顶部，再次执行 scan_internal
            }
            
            // 4. 如果 networks 不是空的，跳出循环
            tracing::info!("Scan successful on attempt {}, found {} networks.", retry_count + 1, networks.len());
            break;
        }

        // 5. 只有在成功扫描后，才启动 AP
        self.start_ap().await?;
        Ok(networks)
    }

    /// 公共方法：连接到指定网络（轮询模式）
    pub async fn connect(&self, req: &ConnectionRequest) -> Result<()> {
        // 停止 AP
        let _ = self.stop_ap().await;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        tracing::debug!("Adding new network...");
        let net_id_str = self.send_cmd("ADD_NETWORK".to_string()).await?;
        let net_id = net_id_str.trim().parse::<u32>()
            .context("Failed to parse ADD_NETWORK response")?;

        tracing::debug!(net_id, "Configuring network...");

        // BUG 修复：使用 Hex 编码 SSID，以支持所有特殊字符
        let ssid_hex = hex::encode(&req.ssid);
        self.send_cmd(format!("SET_NETWORK {} ssid {}", net_id, ssid_hex)).await?;

        // 设置密码或开放网络
        if req.password.is_empty() {
            self.send_cmd(format!("SET_NETWORK {} key_mgmt NONE", net_id)).await?;
        } else {
            // PSK (密码) 仍然使用引号
            self.send_cmd(format!("SET_NETWORK {} psk \"{}\"", net_id, req.password)).await?;
        }

        // 启用网络
        self.send_cmd(format!("ENABLE_NETWORK {}", net_id)).await?;

        // 轮询 STATUS 命令来检测连接状态
        tracing::info!(ssid = %req.ssid, "Connecting... Polling status.");
        let start_time = tokio::time::Instant::now();
        let timeout = Duration::from_secs(30);

        loop {
            // 1. 检查总超时
            if start_time.elapsed() > timeout {
                tracing::error!(ssid = %req.ssid, "Connection timed out after 30s");
                // 超时：清理网络并尝试恢复 AP
                let _ = self.send_cmd(format!("REMOVE_NETWORK {}", net_id)).await;
                let _ = self.start_ap().await;
                return Err(anyhow!("Connection timed out"));
            }

            // 2. 轮询间隔
            tokio::time::sleep(Duration::from_secs(2)).await;

            // 3. 获取状态
            let status_str = match self.send_cmd("STATUS".to_string()).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("Failed to get STATUS, retrying: {}", e);
                    continue;
                }
            };

            // 4. 解析状态，查找 wpa_state
            let mut wpa_state = "";
            for line in status_str.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    if key == "wpa_state" {
                        wpa_state = value;
                        break;
                    }
                }
            }
            
            // 5. 状态机处理
            match wpa_state {
                "COMPLETED" => {
                    tracing::info!(ssid = %req.ssid, "Connection successful (state: COMPLETED)");
                    // 成功后，可以选择保存配置
                    if self.ap_config.wpa_update_config {
                        let _ = self.send_cmd("SAVE_CONFIG".to_string()).await;
                    }

                    // 自动运行 DHCP 客户端
                    tracing::info!("Connection complete. Attempting to run DHCP client (udhcpc)...");
                    let dhcp_status = tokio::process::Command::new("udhcpc")
                        .arg("-i")
                        .arg(&self.ap_config.interface_name)
                        .arg("-q") // 安静模式，减少日志
                        .arg("-n") // 获取 IP 后立即退出，不要作为守护进程
                        .status()
                        .await;

                    if let Ok(status) = dhcp_status {
                        if status.success() {
                            tracing::info!("DHCP client (udhcpc) successfully obtained an IP.");
                        } else {
                            tracing::warn!("DHCP client (udhcpc) exited with an error.");
                        }
                    } else {
                        tracing::error!("Failed to execute 'udhcpc'. Is it installed on this board?");
                    }

                    // 自动退出程序
                    println!("Provisioning complete. Shutting down application.");
                    // 成功退出 (状态码 0)
                    std::process::exit(0);
                }
                "ASSOCIATING" | "ASSOCIATED" | "4WAY_HANDSHAKE" | "GROUP_HANDSHAKE" => {
                    tracing::debug!("Connection in progress (state: {})...", wpa_state);
                    continue; // 还在连接中，继续轮询
                }
                "SCANNING" => {
                    tracing::debug!("wpa_supplicant is scanning...");
                    continue;
                }
                "DISCONNECTED" | "INACTIVE" | "INTERFACE_DISABLED" => {
                    // 刚启动时可能是 DISCONNECTED，给它 5 秒钟反应时间
                    if start_time.elapsed() < Duration::from_secs(5) {
                        tracing::debug!("Waiting for initial connection attempt (state: {})...", wpa_state);
                        continue;
                    }
                    // 5 秒后仍然是 DISCONNECTED，说明连接失败
                    tracing::error!(ssid = %req.ssid, "Connection failed (state: {})", wpa_state);
                    let _ = self.send_cmd(format!("REMOVE_NETWORK {}", net_id)).await;
                    let _ = self.start_ap().await;
                    return Err(anyhow!("Connection failed (state: {})", wpa_state));
                }
                _ => {
                    tracing::warn!("Unknown wpa_state: '{}'", wpa_state);
                    continue;
                }
            }
        }
    }
}
