use crate::Result;
use crate::traits::{ConcurrentBackend, Network, ProvisioningTerminator};
use async_trait::async_trait;
use std::time::Duration;
use tokio::time::sleep;

/// A mock backend for testing purposes.
/// It simulates scanning and connecting without any real hardware interaction.
#[derive(Debug, Default)]
pub struct MockBackend;

impl MockBackend {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ConcurrentBackend for MockBackend {
    async fn enter_provisioning_mode(&self) -> Result<()> {
        println!("🤖 [MockBackend] Entering provisioning mode (simulated).");
        Ok(())
    }

    async fn scan(&self) -> Result<Vec<Network>> {
        println!("🤖 [MockBackend] Scanning for networks...");
        // Simulate a delay
        sleep(Duration::from_secs(2)).await;

        // Return a fixed list of fake networks
        let networks = vec![
            Network {
                ssid: "MyHomeWiFi".to_string(),
                signal: 95,
                security: "WPA3".to_string(),
            },
            Network {
                ssid: "CafeGuest".to_string(),
                signal: 78,
                security: "Open".to_string(),
            },
            Network {
                ssid: "Neighbor's Network".to_string(),
                signal: 55,
                security: "WPA2".to_string(),
            },
            Network {
                ssid: "xfinitywifi".to_string(),
                signal: 88,
                security: "WPA2".to_string(),
            },
            Network {
                ssid: "HiddenNetwork".to_string(),
                signal: 42,
                security: "WPA2".to_string(),
            },
        ];

        println!("🤖 [MockBackend] Found {} networks.", networks.len());
        Ok(networks)
    }
}

#[async_trait]
impl ProvisioningTerminator for MockBackend {
    async fn connect(&self, ssid: &str, password: &str) -> Result<()> {
        println!(
            "🤖 [MockBackend] Attempting to connect to SSID: '{}' with password: '{}'",
            ssid,
            if password.is_empty() { "(empty)" } else { "********" }
        );
        // Simulate a connection delay
        sleep(Duration::from_secs(3)).await;

        // Simulate a failure for a specific network for testing purposes
        if ssid == "xfinitywifi" {
            println!("🤖 [MockBackend] Connection failed to '{}'", ssid);
            Err(crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "Simulated connection failure",
            )))
        } else {
            println!("🤖 [MockBackend] Connection successful to '{}'", ssid);
            Ok(())
        }
    }

    async fn exit_provisioning_mode(&self) -> Result<()> {
        println!("🤖 [MockBackend] Exiting provisioning mode (simulated).");
        Ok(())
    }
}
