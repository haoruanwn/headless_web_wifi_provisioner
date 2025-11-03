use crate::traits::{ProvisioningBackend, Network};
use crate::Result;
use async_trait::async_trait;

/// A backend that communicates with wpa_supplicant via D-Bus.
/// 通过 D-Bus 与 wpa_supplicant 通信的后端。
#[derive(Debug)]
pub struct DbusBackend;

impl DbusBackend {
    pub fn new() -> Self {
        // In the future, this might take a zbus::Connection as an argument
        Self
    }
}

#[async_trait]
impl ProvisioningBackend for DbusBackend {
    async fn scan(&self) -> Result<Vec<Network>> {
        println!("📡 [DbusBackend] Scanning for networks via D-Bus...");
        // TODO: Implement the actual D-Bus call to wpa_supplicant to trigger a scan
        // and retrieve the results.
        
        // For now, return an empty list.
        Ok(vec![])
    }

    async fn connect(&self, ssid: &str, password: &str) -> Result<()> {
        println!(
            "📡 [DbusBackend] Attempting to connect to SSID: '{}' via D-Bus...",
            ssid
        );
        // TODO: Implement the actual D-Bus call to wpa_supplicant to add a new network
        // configuration and trigger a connection.
        let _ = password; // Avoid unused variable warning

        // For now, assume success.
        Ok(())
    }
}
