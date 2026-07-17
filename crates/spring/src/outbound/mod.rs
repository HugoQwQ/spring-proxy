//! Outbound implementations for Spring Proxy.
//!
//! An outbound handles the server-side of a proxied connection:
//! it dials the target, optionally performs protocol-specific handling
//! (MOTD, access control, etc.), and relays data.

use crate::route::ConnectionMetadata;
use crate::stream::Stream;

/// Trait for outbound connections.
#[async_trait::async_trait]
pub trait Outbound: Send + Sync {
    /// Name of this outbound.
    fn name(&self) -> &str;

    /// Handle an inbound connection.
    async fn handle_connection(
        &self,
        conn: Stream,
        metadata: ConnectionMetadata,
    ) -> Result<(), common::Error>;
}

pub mod minecraft;
pub mod plain;
pub use minecraft::MinecraftOutbound;
pub use plain::PlainOutbound;
