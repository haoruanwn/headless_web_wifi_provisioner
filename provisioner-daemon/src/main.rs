use provisioner_core::traits::UiAssetProvider;
use std::sync::Arc;

mod runner;
mod policy;

// 静态分发的前端工厂
fn create_static_frontend() -> Arc<impl UiAssetProvider + 'static> {
    // 编译时验证：确保只选择一个 UI 主题
    const UI_THEME_COUNT: usize = cfg!(feature = "ui_echo_mate") as usize + cfg!(feature = "ui_radxa_x4") as usize;
    const _: () = assert!(UI_THEME_COUNT == 1, "Select exactly ONE UI theme.");
    // reference to silence dead_code when a cfg branch returns early
    let _ = UI_THEME_COUNT;

    #[cfg(feature = "backend_mock")]
    {
        println!("💿 Frontend: Disk Provider selected (Static Dispatch)");
        Arc::new(provisioner_core::frontends::provider_disk::DiskFrontend::new())
    }
    #[cfg(not(feature = "backend_mock"))]
    {
        println!("📦 Frontend: Embed Provider selected (Static Dispatch)");
        Arc::new(provisioner_core::frontends::provider_embed::EmbedFrontend::new())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    println!("🚀 Starting provisioner-daemon...");

    let frontend = create_static_frontend();

    // 将策略分发委托给 policy 模块（按编译时 feature 选择）
    policy::dispatch(frontend).await?;

    Ok(())
}