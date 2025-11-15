//! 启动和清理逻辑

use super::WpaCtrlBackend;
use crate::config::ApConfig;
use crate::structs::Network;
use crate::traits::AudioEvent;
use anyhow::{Result, anyhow, Context};
use std::time::Duration;

impl WpaCtrlBackend {
    /// 在程序启动时执行的清理函数，用于处理上一次退出留下的所有状态。
    /// 这个函数会：
    /// 1. 杀死所有相关的孤儿进程 (wpa_supplicant, hostapd, dnsmasq)。
    /// 2. 清理 /tmp 中所有 wpa_ctrl 相关的客户端套接字。
    /// 3. 清理 wpa_supplicant 服务端套接字。
    /// 4. 启动一个全新的 wpa_supplicant 守护进程。
    pub(super) fn perform_startup_cleanup(config: &ApConfig) -> Result<()> {
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

        // === 新增：重置网络接口状态 ===
        // 这是为了清理 nl80211 驱动中可能残留的"脏"配置
        // 应对 kill -9 或其他非正常关机导致的状态卡死
        // 这个操作在嵌入式 Linux 上非常标准
        tracing::debug!("Resetting interface {} state (down/up)...", config.interface_name);
        let _ = std::process::Command::new("ip")
            .arg("link")
            .arg("set")
            .arg(&config.interface_name)
            .arg("down")
            .status();
        // 等待驱动响应
        std::thread::sleep(Duration::from_millis(500));
        
        let _ = std::process::Command::new("ip")
            .arg("link")
            .arg("set")
            .arg(&config.interface_name)
            .arg("up")
            .status();
        // 等待接口就绪
        std::thread::sleep(Duration::from_millis(500));
        tracing::debug!("Interface state reset complete.");
        // === 新增结束 ===

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
}
