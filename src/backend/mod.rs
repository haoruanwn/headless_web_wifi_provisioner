//! WiFi 供应器后端 - wpa_supplicant 控制套接字实现
//! 
//! 本模块管理 WiFi 网络扫描、AP 模式启动和 WiFi 连接。
//! 核心实现使用 wpa_supplicant 的本机 AP 模式（通过 mode=2）替代 hostapd。

mod commands;
mod parsing;
mod setup;

use crate::config::{ApConfig, AppConfig, load_config_from_toml_str};
use crate::traits::{VoiceNotifier, AudioEvent};
use anyhow::{Result, Context};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};
use wpa_ctrl::{WpaController, WpaControllerBuilder};

// 从配置文件加载总配置
static GLOBAL_APP_CONFIG: Lazy<AppConfig> = Lazy::new(|| {
    const CONFIG_TOML: &str = include_str!("../../configs.toml");
    load_config_from_toml_str(CONFIG_TOML)
});

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
    dnsmasq: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    cmd_ctrl: Arc<Mutex<Option<WpaController>>>,
    ap_net_id: Arc<Mutex<Option<u32>>>,
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
            dnsmasq: Arc::new(tokio::sync::Mutex::new(None)),
            cmd_ctrl: cmd_ctrl_arc,
            ap_net_id: Arc::new(Mutex::new(None)),
            audio_notifier,
        })
    }

    pub fn ap_config(&self) -> Arc<ApConfig> {
        self.ap_config.clone()
    }
}
