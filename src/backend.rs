use crate::config::{ApConfig, AppConfig, load_config_from_toml_str};
use crate::structs::{ConnectionRequest, Network};
use crate::traits::{AudioEvent, VoiceNotifier};
use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::fs;
use tokio::process::Command;
use wpa_ctrl::{WpaController, WpaControllerBuilder};

// 从配置文件加载总配置
static GLOBAL_APP_CONFIG: Lazy<AppConfig> = Lazy::new(|| {
    const CONFIG_TOML: &str = include_str!("../config/wpa_ctrl.toml");
    load_config_from_toml_str(CONFIG_TOML)
});

/// 将 wpa_supplicant 输出中的 `\xHH` 转义序列反转义回原始字节。
/// 主要用于处理扫描结果中 SSID 字段中的汉字等非 ASCII 字符。
fn unescape_wpa_ssid(s: &str) -> Vec<u8> {
    fn hex_val(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(10 + b - b'a'),
            b'A'..=b'F' => Some(10 + b - b'A'),
            _ => None,
        }
    }

    let bs = s.as_bytes();
    let mut out = Vec::with_capacity(bs.len());
    let mut i = 0;
    while i < bs.len() {
        if bs[i] == b'\\' {
            // 处理转义序列
            if i + 1 < bs.len() {
                match bs[i + 1] {
                    b'x' | b'X' => {
                        // 期望后面有两个十六进制字符
                        if i + 3 < bs.len() {
                            let h1 = bs[i + 2];
                            let h2 = bs[i + 3];
                            if let (Some(v1), Some(v2)) = (hex_val(h1), hex_val(h2)) {
                                out.push((v1 << 4) | v2);
                                i += 4;
                                continue;
                            }
                        }
                        // 格式不正确，按字面量保留反斜杠
                        out.push(b'\\');
                        i += 1;
                        continue;
                    }
                    b'\\' => {
                        // 双反斜杠 => 一个反斜杠
                        out.push(b'\\');
                        i += 2;
                        continue;
                    }
                    other => {
                        // 未知的转义序列，保留反斜杠和后面的字符
                        out.push(b'\\');
                        out.push(other);
                        i += 2;
                        continue;
                    }
                }
            } else {
                // 字符串以单个 '\' 结尾
                out.push(b'\\');
                i += 1;
                continue;
            }
        } else {
            out.push(bs[i]);
            i += 1;
        }
    }

    out
}

/// 空的语音播报器（不执行任何操作，用于 audio feature 关闭时或配置不完整时）
struct NullNotifier;

#[async_trait]
impl VoiceNotifier for NullNotifier {
    async fn play(&self, _event: AudioEvent) {
        // Do nothing
    }
}

/// wpa_supplicant 控制套接字后端实现（轮询模式）
pub struct WpaCtrlBackend {
    ap_config: Arc<ApConfig>,
    hostapd: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    dnsmasq: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    cmd_ctrl: Arc<Mutex<Option<WpaController>>>,
    audio_notifier: Arc<dyn VoiceNotifier>,
}

impl WpaCtrlBackend {
    pub fn new() -> Result<Self> {
        let app_config = GLOBAL_APP_CONFIG.clone();
        let ap_config = Arc::new(app_config.ap.clone());

        // 创建 wpa_supplicant 配置文件，使用控制套接字接口
        let update_config_str = if ap_config.wpa_update_config { "1" } else { "0" };
        let wpa_conf_content = format!(
            "ctrl_interface=DIR={} GROUP={}\nupdate_config={}\n",
            ap_config.wpa_ctrl_interface, ap_config.wpa_group, update_config_str
        );
        std::fs::write(&ap_config.wpa_conf_path, wpa_conf_content.as_bytes())
            .context("Failed to write wpa_supplicant config")?;

        tracing::info!("Created wpa_supplicant config at: {}", ap_config.wpa_conf_path);

        // 清理过去的状态，启动一个新的 wpa_supplicant 守护进程
        Self::perform_startup_cleanup(&ap_config)?;

        tracing::debug!("Connecting CMD controller to {}", ap_config.interface_name);
        
        let cmd_ctrl = WpaControllerBuilder::new()
            .open(&ap_config.interface_name)
            .context("Failed to connect WpaController socket. Is wpa_supplicant running?")?;

        let cmd_ctrl_arc = Arc::new(Mutex::new(Some(cmd_ctrl)));

        // === 创建音频 Notifier ===
        let audio_notifier = {
            #[cfg(feature = "audio")]
            {
                if let Some(audio_cfg) = &app_config.audio {
                    tracing::info!("Audio feature enabled.");
                    Arc::new(crate::audio::AplayNotifier::new(Arc::new(audio_cfg.clone()))) as Arc<dyn VoiceNotifier>
                } else {
                    tracing::warn!("Audio feature compiled but no [audio] config found. Disabling audio.");
                    Arc::new(NullNotifier {}) as Arc<dyn VoiceNotifier>
                }
            }
            
            #[cfg(not(feature = "audio"))]
            {
                Arc::new(NullNotifier {}) as Arc<dyn VoiceNotifier>
            }
        };

        Ok(Self {
            ap_config,
            hostapd: Arc::new(tokio::sync::Mutex::new(None)),
            dnsmasq: Arc::new(tokio::sync::Mutex::new(None)),
            cmd_ctrl: cmd_ctrl_arc,
            audio_notifier,
        })
    }

    pub fn ap_config(&self) -> Arc<ApConfig> {
        self.ap_config.clone()
    }

    /// 在程序启动时执行的清理函数，用于处理上一次退出留下的所有状态。
    /// 这个函数会：
    /// 1. 杀死所有相关的孤儿进程 (wpa_supplicant, hostapd, dnsmasq)。
    /// 2. 清理 /tmp 中所有 wpa_ctrl 相关的客户端套接字。
    /// 3. 清理 wpa_supplicant 服务端套接字。
    /// 4. 启动一个全新的 wpa_supplicant 守护进程。
    fn perform_startup_cleanup(config: &ApConfig) -> Result<()> {
        tracing::debug!("Performing robust startup cleanup...");

        // === 1. 杀死所有孤儿进程 ===
        // 我们使用 -9 (SIGKILL) 来确保它们被强行终止
        let _ = std::process::Command::new("killall")
            .arg("-9")
            .arg("wpa_supplicant")
            .status();
        let _ = std::process::Command::new("killall")
            .arg("-9")
            .arg("hostapd")
            .status();
        let _ = std::process::Command::new("killall")
            .arg("-9")
            .arg("dnsmasq")
            .status();
        tracing::debug!("Orphan processes terminated.");
        
        // 短暂等待，确保进程完全退出，端口/资源被释放
        std::thread::sleep(Duration::from_millis(500));


        // 清理/tmp/wpa_ctrl_1
        let wpa_ctrl_1 = std::path::Path::new("/tmp/wpa_ctrl_1");
        if wpa_ctrl_1.exists() {
            match std::fs::remove_file(&wpa_ctrl_1) {
                Ok(_) => tracing::debug!("Removed stale wpa_ctrl socket: {:?}", wpa_ctrl_1),
                Err(e) => tracing::warn!("Failed to remove {:?}: {}", wpa_ctrl_1, e),
            }
        }
        tracing::debug!("All stale wpa_ctrl client sockets cleaned.");


        // === 清理 wpa_supplicant 服务端套接字 ===
        // 例如：/var/run/wpa_supplicant/wlan0
        let socket_path = std::path::Path::new(&config.wpa_ctrl_interface)
            .join(&config.interface_name);
        if socket_path.exists() {
            match std::fs::remove_file(&socket_path) {
                Ok(_) => tracing::debug!("Removed stale server socket: {:?}", socket_path),
                Err(e) => tracing::warn!("Failed to remove stale server socket {:?}: {}", socket_path, e),
            }
        }


        // === 启动一个全新的 wpa_supplicant 守护进程 ===
        tracing::info!("Attempting to start wpa_supplicant daemon...");
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

                // 发送请求
                ctrl.request(WpaControlReq::raw(&cmd))
                    .map_err(|e| anyhow!("wpa_ctrl request failed: {}", e))?;

                // 健壮地接收回复：循环读取，丢弃所有非请求（unsolicited）消息，
                // 并在遇到失败回复时返回错误，直至得到目标回复。
                loop {
                    match ctrl.recv() {
                        Ok(Some(msg)) => {
                            // 丢弃守护进程主动推送的非请求消息
                            if msg.is_unsolicited() {
                                tracing::debug!("WPA_CMD_RECV (unsolicited): {}", msg.raw);
                                continue;
                            }

                            // 如果这是一个明确的失败回复，返回错误
                            // crate 提供 as_fail() 来判断失败类型
                            if msg.as_fail().is_some() {
                                tracing::error!("WPA_CMD_RECV (FAIL): {}", msg.raw);
                                return Err(anyhow!("WPA command failed: {}", msg.raw));
                            }

                            // 否则这是我们期望的回复，返回给上层
                            tracing::debug!("WPA_CMD_RECV (OK/DATA): {}", msg.raw);
                            return Ok(msg.raw.to_string());
                        }
                        Ok(None) => {
                            return Err(anyhow!("No response received"));
                        }
                        Err(e) => {
                            return Err(anyhow!("recv failed: {}", e));
                        }
                    }
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

            // wpa_supplicant 对包含非 ASCII 字节的 SSID 会以 `\xHH` 转义序列输出。
            // 这里将其反转义回原始字节，然后尝试用 UTF-8 解码（使用 from_utf8_lossy 保持健壮性）。
            let raw_ssid = parts[4];
            let ssid_bytes = unescape_wpa_ssid(raw_ssid);
            let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();

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
        self.audio_notifier.play(AudioEvent::ApStarted).await;
        Ok(networks)
    }

    /// 公共方法：连接到指定网络（轮询模式）
    pub async fn connect(&self, req: &ConnectionRequest) -> Result<()> {
        // 停止 AP
        let _ = self.stop_ap().await;
        self.audio_notifier.play(AudioEvent::ConnectionStarted).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        tracing::debug!("Adding new network...");
        let net_id_str = self.send_cmd("ADD_NETWORK".to_string()).await?;
        let net_id = net_id_str.trim().parse::<u32>()
            .context("Failed to parse ADD_NETWORK response")?;

        tracing::debug!(net_id, "Configuring network...");

        // 使用 Hex 编码 SSID，以支持所有特殊字符
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
                self.audio_notifier.play(AudioEvent::ConnectionFailed).await;
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

                    // 播放连接成功的音频
                    self.audio_notifier.play(AudioEvent::ConnectionSuccess).await;
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

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
                    self.audio_notifier.play(AudioEvent::ConnectionFailed).await;
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
