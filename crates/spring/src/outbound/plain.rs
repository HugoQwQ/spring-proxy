//! Plain TCP outbound.
//!
//! Dials the target address and relays data bidirectionally.

use crate::outbound::Outbound;
use crate::relay::{self, RelayConfig};
use crate::route::ConnectionMetadata;
use crate::stream::Stream;

/// A plain TCP outbound that relays to a target address.
pub struct PlainOutbound {
    name: String,
    target_address: String,
    target_port: u16,
}

impl PlainOutbound {
    /// Create a new plain outbound.
    pub fn new(
        name: impl Into<String>,
        target_address: impl Into<String>,
        target_port: u16,
    ) -> Self {
        Self {
            name: name.into(),
            target_address: target_address.into(),
            target_port,
        }
    }
}

#[async_trait::async_trait]
impl Outbound for PlainOutbound {
    fn name(&self) -> &str {
        &self.name
    }

    async fn handle_connection(
        &self,
        conn: Stream,
        metadata: ConnectionMetadata,
    ) -> Result<(), common::Error> {
        // Determine target
        let target_host = if !metadata
            .minecraft
            .as_ref()
            .map(|m| m.rewritten_destination.as_str())
            .unwrap_or("")
            .is_empty()
        {
            metadata
                .minecraft
                .as_ref()
                .unwrap()
                .rewritten_destination
                .clone()
        } else if !self.target_address.is_empty() {
            self.target_address.clone()
        } else {
            metadata
                .minecraft
                .as_ref()
                .map(|m| m.origin_destination.clone())
                .unwrap_or_default()
        };

        let target_port = if metadata
            .minecraft
            .as_ref()
            .map(|m| m.rewritten_port)
            .unwrap_or(0)
            > 0
        {
            metadata.minecraft.as_ref().unwrap().rewritten_port
        } else if self.target_port > 0 {
            self.target_port
        } else {
            metadata
                .minecraft
                .as_ref()
                .map(|m| m.origin_port)
                .unwrap_or(25565)
        };

        let target_addr = format!("{}:{}", target_host, target_port);

        log::info!("[{}] Dialing {}", self.name, target_addr);

        let target = async_net::TcpStream::connect(&target_addr).await?;

        relay::relay(conn, target, RelayConfig::default()).await?;

        Ok(())
    }
}
