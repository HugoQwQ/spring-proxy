//! TCP service for Spring Proxy.
//!
//! Listens on a port, accepts connections, performs IP access control,
//! sniffs Minecraft handshakes, and dispatches to the router or a legacy outbound.

use std::net::SocketAddr;
use std::sync::Arc;

use common::access::{AccessMode, check};
use common::set::StringSet;

use crate::config::ServiceConfig;
use crate::outbound::Outbound;
use crate::route::{ConnectionMetadata, Router};
use crate::stream::Stream;

/// A running TCP service.
pub struct Service {
    config: ServiceConfig,
    router: Arc<Router>,
    legacy_outbound: Option<Arc<dyn Outbound>>,
    ip_access_lists: Vec<StringSet>,
    ip_access_mode: AccessMode,
}

impl Service {
    /// Create a new service from configuration.
    pub fn new(
        config: ServiceConfig,
        router: Arc<Router>,
        legacy_outbound: Option<Arc<dyn Outbound>>,
    ) -> Self {
        let ip_access_mode = AccessMode::from_str(&config.ip_access.mode);
        Self {
            config,
            router,
            legacy_outbound,
            ip_access_lists: Vec::new(),
            ip_access_mode,
        }
    }

    /// Load IP access control lists from the router.
    pub async fn load_access_lists(&mut self) -> Result<(), common::Error> {
        if self.ip_access_mode != AccessMode::Default {
            self.ip_access_lists = self
                .router
                .find_lists_by_tag(&self.config.ip_access.list_tags)?;
        }
        Ok(())
    }

    /// Start the service: bind and accept connections.
    pub async fn start(&self) -> Result<(), common::Error> {
        let listen_addr: SocketAddr = format!("0.0.0.0:{}", self.config.listen)
            .parse()
            .map_err(|e| common::Error::Protocol(format!("invalid listen address: {e}")))?;

        let listener = async_net::TcpListener::bind(listen_addr).await?;
        log::info!(
            "Service '{}' listening on {}",
            self.config.name,
            listen_addr
        );

        loop {
            let (conn, _) = listener.accept().await?;
            let service = self.clone();
            smol::spawn(async move {
                if let Err(e) = service.handle_connection(conn).await {
                    log::warn!("Connection error in service '{}': {e}", service.config.name);
                }
            })
            .detach();
        }
    }

    /// Handle a single inbound connection.
    async fn handle_connection(&self, conn: async_net::TcpStream) -> Result<(), common::Error> {
        let peer = conn.peer_addr()?;
        let ip_str = peer.ip().to_string();

        // IP access control
        if self.ip_access_mode != AccessMode::Default {
            if !check(&self.ip_access_lists, self.ip_access_mode, &ip_str) {
                log::warn!(
                    "Service '{}' rejected connection from {} (IP access control)",
                    self.config.name,
                    ip_str
                );
                return Ok(());
            }
        }

        log::info!(
            "Service '{}' new connection from {}",
            self.config.name,
            peer
        );

        // Build metadata
        let mut metadata = ConnectionMetadata {
            service_name: self.config.name.clone(),
            source_addr: peer,
            ..ConnectionMetadata::default()
        };

        // Sniff Minecraft handshake (always attempt, for both legacy and router modes)
        let mut mc_stream = protocol::MinecraftStream::new(conn);
        let stream = match mc_stream.sniff_full().await {
            Ok(mc_meta) => {
                metadata.minecraft = Some(mc_meta);
                Stream::minecraft(mc_stream)
            }
            Err(e) => {
                log::debug!(
                    "Service '{}' Minecraft sniff error from {}: {e}",
                    self.config.name,
                    peer
                );
                // Not a Minecraft connection — unwrap to plain TCP so that
                // any partially-read bytes don't corrupt the relay.
                Stream::plain(mc_stream.into_inner())
            }
        };

        // Legacy mode: directly handle with the outbound
        if let Some(ref outbound) = self.legacy_outbound {
            outbound.handle_connection(stream, metadata).await?;
            return Ok(());
        }

        // Router mode: pass to router
        self.router.handle_connection(stream, metadata).await;
        Ok(())
    }
}

impl Clone for Service {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            router: self.router.clone(),
            legacy_outbound: self.legacy_outbound.clone(),
            ip_access_lists: self.ip_access_lists.clone(),
            ip_access_mode: self.ip_access_mode,
        }
    }
}
