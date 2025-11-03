// Placeholder for a backend that interacts with systemd-networkd.
// This demonstrates the extensibility of the backend architecture.
// 这是一个占位符，表示一个与 systemd-networkd 交互的后端。

use crate::traits::{ProvisioningBackend, Network};
use crate::Result;
use async_trait::async_trait;

#[derive(Debug)]
pub struct SystemdNetworkdBackend;

impl SystemdNetworkdBackend {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProvisioningBackend for SystemdNetworkdBackend {
    async fn scan(&self) -> Result<Vec<Network>> {
        println!("🤖 [SystemdNetworkdBackend] Scanning not yet implemented.");
        unimplemented!("This backend is a placeholder and does not yet implement scanning.")
    }

    async fn connect(&self, _ssid: &str, _password: &str) -> Result<()> {
        println!("🤖 [SystemdNetworkdBackend] Connecting not yet implemented.");
        unimplemented!("This backend is a placeholder and does not yet implement connecting.")
    }
}
