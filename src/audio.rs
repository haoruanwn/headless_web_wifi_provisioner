#![cfg(feature = "audio")]

use crate::config::AudioConfig;
use crate::traits::{AudioEvent, VoiceNotifier};
use async_trait::async_trait;
use rust_embed::RustEmbed;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use std::process::Stdio;

/// 嵌入 audio/ 目录中的音频文件
#[derive(RustEmbed)]
#[folder = "audio/"]
struct AudioAsset;

/// 使用 aplay 播放音频的实现
pub struct AplayNotifier {
    config: Arc<AudioConfig>,
}

impl AplayNotifier {
    /// 创建新的 AplayNotifier
    pub fn new(config: Arc<AudioConfig>) -> Self {
        Self { config }
    }
}

#[async_trait]
impl VoiceNotifier for AplayNotifier {
    async fn play(&self, event: AudioEvent) {
        let config = self.config.clone();
        
        // 根据事件类型获取对应的音频文件名
        let file_name = match event {
            AudioEvent::ApStarted => &config.files.ap_started,
            AudioEvent::ConnectionStarted => &config.files.connection_started,
            AudioEvent::ConnectionSuccess => &config.files.connection_success,
            AudioEvent::ConnectionFailed => &config.files.connection_failed,
        };

        // 从嵌入式资源中获取音频数据
        let asset = match AudioAsset::get(file_name) {
            Some(asset) => asset,
            None => {
                tracing::error!("Audio asset not found: {}", file_name);
                return;
            }
        };
        let audio_data = asset.data;

        // 启动一个新的异步任务来播放音频，不阻塞主逻辑
        tokio::spawn(async move {
            tracing::debug!("Playing audio: {:?}", event);
            
            let mut cmd = Command::new("aplay");
            
            // 设置声卡设备
            // 如果是"auto"，则使用默认设备
            if config.device != "auto" {
                cmd.arg("-D").arg(&config.device);
            }
            
            // 设置标准输入为管道，丢弃输出
            cmd.stdin(Stdio::piped())
               .stdout(Stdio::null())
               .stderr(Stdio::null());
            
            // 启动 aplay 进程
            let mut child = match cmd.spawn() {
                Ok(child) => child,
                Err(e) => {
                    tracing::error!("Failed to spawn aplay: {}", e);
                    return;
                }
            };

            // 获取 stdin 句柄并写入音频数据
            if let Some(mut stdin) = child.stdin.take() {
                if let Err(e) = stdin.write_all(audio_data.as_ref()).await {
                    tracing::error!("Failed to pipe audio data to aplay: {}", e);
                }
                // 关闭 stdin 以让 aplay 知道数据已结束
                drop(stdin);
            }

            // 等待 aplay 进程完成
            if let Err(e) = child.wait().await {
                tracing::error!("aplay process failed: {}", e);
            } else {
                tracing::debug!("Audio playback finished.");
            }
        });
    }
}
