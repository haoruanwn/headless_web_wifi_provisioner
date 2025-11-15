use anyhow::Result;
use provisioner::run_provisioner;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 初始化日志（这是入口点的职责）
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // 2. 调用库的核心逻辑
    if let Err(e) = run_provisioner().await {
        // 3. 处理顶层错误
        tracing::error!("❌ Provisioner failed: {}", e);
        // 在这里处理退出码是合适的
        std::process::exit(1);
    }

    Ok(())
}
