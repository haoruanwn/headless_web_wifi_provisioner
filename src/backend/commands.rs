use super::WpaCtrlBackend;
use super::parsing::{parse_scan_results, channel_to_frequency};
use crate::structs::{Network, ConnectionRequest};
use crate::traits::AudioEvent;
use anyhow::{Result, anyhow, Context};
use std::time::Duration;
use tokio::process::Command;

impl WpaCtrlBackend {
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

    /// 内部扫描方法（轮询模式）
    pub(super) async fn scan_internal(&self) -> Result<Vec<Network>> {
        tracing::debug!("Sending SCAN command...");
        self.send_cmd("SCAN".to_string()).await?;

        // 固定等待 10 秒以确保扫描完成
        tracing::debug!("Waiting 10 seconds for scan results...");
        tokio::time::sleep(Duration::from_secs(10)).await;
        
        tracing::debug!("Scan wait complete, fetching results.");
        let results_str = self.send_cmd("SCAN_RESULTS".to_string()).await?;
        parse_scan_results(&results_str)
    }

    /// 启动 AP 模式
    pub(super) async fn start_ap(&self) -> Result<()> {
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

        // 使用 wpa_supplicant 控制接口创建 AP 网络（替代 hostapd）
        if let Err(e) = self.start_ap_internal().await {
            tracing::error!("Failed to start AP via wpa_supplicant: {}", e);
            return Err(e);
        }

        // 启动 dnsmasq（IP 层）以提供 DHCP 服务
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
        tracing::info!("AP started successfully on {} (via wpa_supplicant)", self.ap_config.interface_name);
        Ok(())
    }

    /// 使用 wpa_supplicant 控制接口创建并启用一个 AP 网络
    async fn start_ap_internal(&self) -> Result<()> {
        tracing::debug!("Creating AP network via wpa_supplicant");

        // 1. ADD_NETWORK
        let add_resp = self.send_cmd("ADD_NETWORK".to_string()).await?;
        let net_id = add_resp.trim().parse::<u32>().context("Failed to parse ADD_NETWORK response")?;

        tracing::debug!(net_id, "Configuring AP network id");

        // 2. 设置 mode=2 (AP) - 任何失败都会立即返回错误
        self.send_cmd(format!("SET_NETWORK {} mode 2", net_id)).await?;

        // 3. 设置 SSID（使用 hex 编码以支持任意字符）
        let ssid_hex = hex::encode(&self.ap_config.ssid);
        self.send_cmd(format!("SET_NETWORK {} ssid {}", net_id, ssid_hex)).await?;

        // 4. 设置安全性
        if self.ap_config.psk.is_empty() {
            self.send_cmd(format!("SET_NETWORK {} key_mgmt NONE", net_id)).await?;
        } else {
            // 设置 WPA 协议和加密算法
            // wpa_supplicant 需要明确知道使用 WPA2 (RSN) 还是 WPA1
            if self.ap_config.hostapd_wpa == 2 {
                // RSN = WPA2
                self.send_cmd(format!("SET_NETWORK {} proto RSN", net_id)).await?; 
            } else if self.ap_config.hostapd_wpa == 1 {
                // WPA = WPA1
                self.send_cmd(format!("SET_NETWORK {} proto WPA", net_id)).await?;
            } else {
                // 混合模式或其他
                self.send_cmd(format!("SET_NETWORK {} proto WPA RSN", net_id)).await?;
            }

            // 设置密钥管理
            self.send_cmd(format!(
                "SET_NETWORK {} key_mgmt {}",
                net_id, self.ap_config.hostapd_wpa_key_mgmt
            ))
            .await?;

            // 设置加密套件 (CCMP/TKIP 等)
            self.send_cmd(format!(
                "SET_NETWORK {} pairwise {}",
                net_id, self.ap_config.hostapd_wpa_pairwise
            ))
            .await?;

            // 设置密码
            self.send_cmd(format!("SET_NETWORK {} psk \"{}\"", net_id, self.ap_config.psk)).await?;
        }

        // 5. 设置频率/信道（如果配置了）
        if let Some(freq) = channel_to_frequency(self.ap_config.hostapd_channel, &self.ap_config.hostapd_hw_mode) {
            // 我们尝试设置频率，但如果失败也不 panic，因为这通常是非致命的。
            // 某些 wpa_supplicant/driver 组合不支持在 AP 模式下设置频率。
            let cmd = format!("SET_NETWORK {} freq {}", net_id, freq);
            match self.send_cmd(cmd).await {
                Ok(_) => {
                    tracing::debug!(
                        channel = self.ap_config.hostapd_channel,
                        freq = freq,
                        "Successfully set AP frequency"
                    );
                }
                Err(e) => {
                    // 仅记录警告，不中断 AP 启动
                    tracing::warn!(
                        channel = self.ap_config.hostapd_channel,
                        freq = freq,
                        error = %e,
                        "Failed to set AP frequency (command failed). This is often non-fatal, driver will auto-select channel."
                    );
                }
            }
        } else {
            tracing::warn!(
                channel = self.ap_config.hostapd_channel,
                hw_mode = %self.ap_config.hostapd_hw_mode,
                "Could not map channel to frequency, using driver default"
            );
        }

        // 6. 启用网络
        self.send_cmd(format!("ENABLE_NETWORK {}", net_id)).await?;

        // 7. 记录 net id 以便后续移除
        let mut guard = self.ap_net_id.lock().unwrap();
        *guard = Some(net_id);
        tracing::info!("AP network {} enabled via wpa_supplicant", net_id);

        Ok(())
    }

    /// 停止 AP 模式
    pub(super) async fn stop_ap(&self) -> Result<()> {
        // 杀死我们启动的进程
        if let Some(mut child) = self.dnsmasq.lock().await.take() {
            let _ = child.kill().await;
        }

        // 如果通过 wpa_supplicant 创建了 AP 网络，尝试移除它
        // 取出当前记录的 network id（先释放锁，再执行 await）
        let maybe_net_id = {
            let mut guard = self.ap_net_id.lock().unwrap();
            let nid = *guard;
            *guard = None;
            nid
        };
        if let Some(net_id) = maybe_net_id {
            let _ = self.send_cmd(format!("REMOVE_NETWORK {}", net_id)).await;
            tracing::debug!("Removed AP network id {}", net_id);
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

        tracing::info!("AP stopped on {}", self.ap_config.interface_name);
        Ok(())
    }

    /// 公开方法：连接到指定网络（轮询模式）
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
