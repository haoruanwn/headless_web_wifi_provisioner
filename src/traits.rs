use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;

/// 一个提供 UI 静态资产的通用 Trait。
///
/// `Send + Sync` 约束是必须的，因为它将被用于 Axum 的共享状态 (State) 中。
#[async_trait]
pub trait UiAssetProvider: Send + Sync {
    /// 根据路径获取一个资产。
    ///
    /// 返回元组 (数据, Mime类型字符串)
    async fn get_asset(&self, path: &str) -> Result<(Cow<'static, [u8]>, String)>;
}

// ============= 音频相关 Trait =============

/// 语音播报的事件类型
#[derive(Debug, Clone, Copy)]
pub enum AudioEvent {
    /// AP 已启动
    ApStarted,
    /// 连接已开始
    ConnectionStarted,
    /// 连接成功
    ConnectionSuccess,
    /// 连接失败
    ConnectionFailed,
}

/// 一个提供语音播报的通用 Trait
#[async_trait]
pub trait VoiceNotifier: Send + Sync {
    /// 播报一个事件
    ///
    /// 这应该是一个 "fire and forget" 操作，
    /// 不应阻塞当前的异步任务。
    async fn play(&self, event: AudioEvent);
}