use provisioner_core::traits::UiAssetProvider;
use std::sync::Arc;

/// On-Start 策略：程序启动时立即进入配网模式
pub async fn run(frontend: Arc<impl UiAssetProvider + 'static>) -> anyhow::Result<()> {
    println!("🚀 Policy: On-Start. Entering provisioning mode immediately.");
    crate::runner::run_provisioning_server(frontend).await?;
    Ok(())
}
