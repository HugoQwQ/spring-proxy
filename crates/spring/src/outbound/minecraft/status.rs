//! Minecraft status (MOTD) handler.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use async_lock::RwLock;
use common::Error;
use protocol::{MinecraftMetadata, motd};
use smol::io::{AsyncReadExt, AsyncWriteExt};

use crate::config::MinecraftOutboundConfig;
use crate::outbound::minecraft::packets;
use crate::relay;
use crate::route::ConnectionMetadata;
use crate::stream::Stream;

/// Handles Minecraft status (server list ping) requests.
pub(crate) struct StatusHandler {
    name: String,
    target_address: String,
    target_port: u16,
    config: Arc<RwLock<MinecraftOutboundConfig>>,
    online_count: Arc<AtomicI32>,
}

impl StatusHandler {
    pub(crate) fn new(
        name: impl Into<String>,
        target_address: impl Into<String>,
        target_port: u16,
        config: Arc<RwLock<MinecraftOutboundConfig>>,
        online_count: Arc<AtomicI32>,
    ) -> Self {
        Self {
            name: name.into(),
            target_address: target_address.into(),
            target_port,
            config,
            online_count,
        }
    }

    /// Handle a STATUS request.
    pub(crate) async fn handle(
        &self,
        mut stream: Stream,
        metadata: &ConnectionMetadata,
    ) -> Result<(), Error> {
        let mc = metadata.minecraft.as_ref().unwrap();
        stream.consume_peek(mc.sniff_position);

        let config = self.config.read().await;
        if config.motd_favicon.is_empty() && config.motd_description.is_empty() {
            // No custom MOTD: proxy from backend.
            drop(config);
            self.proxy_from_backend(stream, metadata).await
        } else {
            // Custom MOTD: consume the status request bytes.
            drop(config);
            let mut skip = [0u8; 2];
            stream.read_exact(&mut skip).await?;
            self.send_custom_motd(stream, mc).await
        }
    }

    /// Proxy MOTD from the backend server.
    async fn proxy_from_backend(
        &self,
        mut stream: Stream,
        metadata: &ConnectionMetadata,
    ) -> Result<(), Error> {
        let mc = metadata.minecraft.as_ref().unwrap();

        let hostname = if mc.rewritten_destination.is_empty() {
            mc.clean_origin_destination().to_string()
        } else {
            mc.rewritten_destination.clone()
        };
        let port = if mc.rewritten_port > 0 {
            mc.rewritten_port
        } else {
            mc.origin_port
        };

        let target_addr = format!("{}:{}", self.target_address, self.target_port);

        log::debug!(
            "[{}] Resolving backend for status: {}",
            self.name,
            target_addr
        );
        let mut target = match async_net::TcpStream::connect(&target_addr).await {
            Ok(t) => {
                if let Ok(peer) = t.peer_addr() {
                    log::debug!(
                        "[{}] Connected to {} (resolved: {})",
                        self.name,
                        target_addr,
                        peer
                    );
                }
                t
            }
            Err(e) => {
                log::warn!(
                    "[{}] Failed to connect to backend {} for status: {e}",
                    self.name,
                    target_addr
                );
                let msg = protocol::message::Message {
                    color: "#FF5555".into(),
                    text: format!("§cUnable to connect to backend server.\n§7{e}"),
                    ..protocol::message::Message::default()
                };
                return super::send_kick(&mut stream, &msg).await;
            }
        };

        let packet_buf = packets::build_handshake_packet(
            mc.protocol_version,
            &hostname,
            port,
            1, // intent = status
        )?;

        // Append post-handshake bytes (status request)
        let mut full_buf = packet_buf;
        if let Some(post) = stream.post_handshake_bytes(0) {
            full_buf.extend_from_slice(post);
        } else {
            full_buf.extend_from_slice(&[0x01, 0x00]);
        }

        target.write_all(&full_buf).await?;
        stream.consume_peek(usize::MAX);

        relay::relay(stream, target, crate::relay::RelayConfig::default()).await
    }

    /// Send a custom MOTD response.
    async fn send_custom_motd(
        &self,
        mut stream: Stream,
        mc: &MinecraftMetadata,
    ) -> Result<(), Error> {
        let config = self.config.read().await;

        let online = if config.online_count.online < 0 {
            self.online_count.load(Ordering::Relaxed)
        } else {
            config.online_count.online
        };

        let motd = motd::generate_motd(
            mc.protocol_version,
            &format!("Spring Proxy {}", env!("CARGO_PKG_VERSION")),
            &config.motd_description,
            config.online_count.max,
            online,
            Some(config.motd_favicon.as_str()).filter(|s| !s.is_empty()),
            None,
        );

        let motd_json = std::str::from_utf8(&motd).unwrap_or("{}");
        let packet_buf = packets::build_status_response_packet(motd_json)?;
        stream.write_all(&packet_buf).await?;

        match config.ping_mode.as_str() {
            "disconnect" => {
                // Close immediately
            }
            "0ms" => {
                let pkt = packets::build_ping_pong_packet(0)?;
                stream.write_all(&pkt).await?;
            }
            _ => {
                drop(config);
                let mut buf = [0u8; 32];
                let n = stream.read(&mut buf).await?;
                if n > 0 {
                    stream.write_all(&buf[..n]).await?;
                }
            }
        }

        log::info!("[{}] Responded MOTD to status request", self.name);
        Ok(())
    }
}
