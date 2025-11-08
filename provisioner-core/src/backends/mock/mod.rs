use crate::Result;
use crate::traits::{ConcurrentBackend, TdmBackend, Network, PolicyCheck};
use async_trait::async_trait;
use std::time::Duration;
use std::net::{SocketAddr, Ipv4Addr};
use tokio::time::sleep;

/// A mock backend for testing purposes.
/// It simulates scanning and connecting without any real hardware interaction.
#[derive(Debug, Default)]
pub struct MockConcurrentBackend;

impl MockConcurrentBackend {
    pub fn new() -> Self {
        Self
    }
}

 

#[async_trait]
impl ConcurrentBackend for MockConcurrentBackend {
    fn get_bind_address(&self) -> SocketAddr {
        SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 3000)
    }
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

#[async_trait]
impl PolicyCheck for MockConcurrentBackend {
    async fn is_connected(&self) -> crate::Result<bool> {
        println!("👻 [MockBackend] Checking connection status (simulated, returning false)");
        Ok(false)
    }
}

// ---------------- TDM Mock Backend -----------------
#[derive(Debug, Default)]
pub struct MockTdmBackend;

impl MockTdmBackend {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl TdmBackend for MockTdmBackend {
    fn get_bind_address(&self) -> SocketAddr {
        SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), 3000)
    }
    async fn enter_provisioning_mode_with_scan(&self) -> Result<Vec<Network>> {
        println!("🤖 [MockTdmBackend] Enter provisioning (scan then AP) simulated");
        // Simulate scan delay
        sleep(Duration::from_secs(2)).await;
        Ok(vec![
            Network { ssid: "TDM_Network_A".into(), signal: 80, security: "WPA2".into() },
            Network { ssid: "TDM_Network_B".into(), signal: 60, security: "Open".into() },
        ])
    }

    async fn connect(&self, ssid: &str, _password: &str) -> Result<()> {
        println!("🤖 [MockTdmBackend] Connect (terminating) to '{}' simulated", ssid);
        sleep(Duration::from_secs(1)).await;
        Ok(())
    }

    async fn exit_provisioning_mode(&self) -> Result<()> {
        println!("🤖 [MockTdmBackend] Exit provisioning (AP down) simulated");
        Ok(())
    }
}

#[async_trait]
impl PolicyCheck for MockTdmBackend {
    async fn is_connected(&self) -> crate::Result<bool> {
        // Always false for deterministic policy testing
        Ok(false)
    }
}
