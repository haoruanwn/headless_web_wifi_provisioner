use provisioner_core::traits::UiAssetProvider;
use std::sync::Arc;
use tokio::process::Command;

/// 守护进程策略：仅当指定接口未连接时才进入配网模式
#[allow(dead_code)]
pub async fn run(frontend: Arc<impl UiAssetProvider + 'static>) -> anyhow::Result<()> {
    println!("🚀 Policy: Daemon (If-Disconnected).");
    // 注意：此检查特定于 Linux 平台并依赖于 wpa_cli
    if !check_if_already_connected("wlan0").await {
        crate::runner::run_provisioning_server(frontend).await?;
    }
    Ok(())
}

#[allow(dead_code)]
async fn check_if_already_connected(iface: &str) -> bool {
    println!("🛡️ Daemon Policy: Checking network status on {}...", iface);
    let output = Command::new("wpa_cli").arg("-i").arg(iface).arg("status").output().await;

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("wpa_state=COMPLETED") && stdout.contains("ip_address=") {
                let ip = stdout
                    .lines()
                    .find(|line| line.starts_with("ip_address="))
                    .unwrap_or("ip_address=N/A");
                println!("🛡️ Daemon Policy: WiFi is ALREADY CONNECTED ({}). Provisioner will not start.", ip);
                true
            } else {
                println!(
                    "🛡️ Daemon Policy: WiFi is NOT connected (State: {}). Starting provisioner...",
                    stdout
                        .lines()
                        .find(|line| line.starts_with("wpa_state="))
                        .unwrap_or("wpa_state=UNKNOWN")
                );
                false
            }
        }
        Err(e) => {
            println!("🛡️ Daemon Policy: wpa_cli failed ({}). Assuming NOT connected. Starting provisioner...", e);
            false
        }
    }
}
