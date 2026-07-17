//! Minecraft outbound proxy.
//!
//! Handles the server-side of Minecraft connections:
//! - Status (MOTD) responses — custom or proxied from backend
//! - Login handling — access control, player limits, backend relay
//! - Hostname rewriting and FML (Forge Mod Loader) suffix handling

use std::sync::atomic::{AtomicI32, Ordering};

use async_lock::RwLock;
use common::{BytesMut, Error, VarInt, access::AccessMode};
use protocol::kick::{generate_kick_message, generate_player_number_limit_exceeded_message};
use protocol::motd::{DEFAULT_MOTD_FAVICON, generate_motd};
use protocol::{MinecraftMetadata, packet};
use smol::io::{AsyncReadExt, AsyncWriteExt};

use crate::config::MinecraftOutboundConfig;
use crate::outbound::Outbound;
use crate::relay;
use crate::route::ConnectionMetadata;
use crate::stream::Stream;

/// A Minecraft-specific outbound.
pub struct MinecraftOutbound {
    name: String,
    target_address: String,
    target_port: u16,
    config: RwLock<MinecraftOutboundConfig>,
    hostname_access_lists: Vec<common::set::StringSet>,
    name_access_lists: Vec<common::set::StringSet>,
    online_count: AtomicI32,
}

impl MinecraftOutbound {
    /// Create a new Minecraft outbound.
    pub fn new(
        name: impl Into<String>,
        target_address: impl Into<String>,
        target_port: u16,
        mut config: MinecraftOutboundConfig,
    ) -> Self {
        let name = name.into();
        let target_address = target_address.into();

        if config.motd_favicon == "{DEFAULT_MOTD}" {
            config.motd_favicon = DEFAULT_MOTD_FAVICON.into();
        }

        // Replace placeholders in MOTD description
        config.motd_description = config
            .motd_description
            .replace(
                "{INFO}",
                &format!("Spring Proxy {}", env!("CARGO_PKG_VERSION")),
            )
            .replace("{NAME}", &name)
            .replace("{HOST}", &target_address)
            .replace("{PORT}", &target_port.to_string());

        Self {
            name,
            target_address,
            target_port,
            config: RwLock::new(config),
            hostname_access_lists: Vec::new(),
            name_access_lists: Vec::new(),
            online_count: AtomicI32::new(0),
        }
    }

    /// Load access control lists.
    pub fn set_access_lists(
        &mut self,
        hostname: Vec<common::set::StringSet>,
        name: Vec<common::set::StringSet>,
    ) {
        self.hostname_access_lists = hostname;
        self.name_access_lists = name;
    }

    /// Handle a STATUS (MOTD) request.
    async fn handle_status(
        &self,
        mut stream: Stream,
        metadata: &ConnectionMetadata,
    ) -> Result<(), Error> {
        let mc = metadata.minecraft.as_ref().unwrap();
        let config = self.config.read().await;

        // Consume the handshake bytes from the stream's peek buffer
        // so they are not replayed to the backend or misread here.
        stream.consume_peek(mc.sniff_position);

        if config.motd_favicon.is_empty() && config.motd_description.is_empty() {
            // No custom MOTD: proxy from backend server.
            // Leave the status request in the peek buffer so it can be
            // forwarded to the backend together with the rewritten handshake.
            drop(config);
            self.proxy_status_from_backend(stream, metadata).await
        } else {
            // Custom MOTD: we don't need the status request, consume it.
            drop(config);
            let mut skip = [0u8; 2];
            stream.read_exact(&mut skip).await?;
            self.send_custom_motd(stream, mc).await
        }
    }

    /// Proxy MOTD from the backend server.
    async fn proxy_status_from_backend(
        &self,
        mut stream: Stream,
        metadata: &ConnectionMetadata,
    ) -> Result<(), Error> {
        let mc = metadata.minecraft.as_ref().unwrap();
        let config = self.config.read().await;

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
        drop(config);

        log::debug!("[{}] Resolving backend for status: {}", self.name, target_addr);
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
                    text: format!(
                        "§cUnable to connect to backend server.\n§7{}",
                        e
                    ),
                    ..protocol::message::Message::default()
                };
                return self.send_kick(stream, &msg, "backend unreachable").await;
            }
        };

        // Build rewritten handshake packet for backend
        let mut handshake = BytesMut::new();
        packet::write_byte(&mut handshake, 0x00); // packet ID
        let mut tmp = [0u8; 5];
        let n = VarInt(mc.protocol_version).encode(&mut tmp)?;
        handshake.extend_from_slice(&tmp[..n]);
        packet::write_string(&mut handshake, &hostname);
        packet::write_u16_be(&mut handshake, port);
        packet::write_byte(&mut handshake, 1); // intent = status

        let mut packet_buf = BytesMut::new();
        let n = VarInt(handshake.len() as i32).encode(&mut tmp)?;
        packet_buf.extend_from_slice(&tmp[..n]);
        packet_buf.extend_from_slice(&handshake);

        // The handshake has already been consumed by handle_status.
        // What remains in the peek buffer is the post-handshake bytes.
        if let Some(post) = stream.post_handshake_bytes(0) {
            packet_buf.extend_from_slice(post);
        } else {
            // Fallback: hard-coded status request
            packet_buf.extend_from_slice(&[0x01, 0x00]);
        }

        target.write_all(&packet_buf).await?;

        // Consume all peeked bytes so relay does not replay them.
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

        let motd = generate_motd(
            mc.protocol_version,
            &format!("Spring Proxy {}", env!("CARGO_PKG_VERSION")),
            &config.motd_description,
            config.online_count.max,
            online,
            Some(config.motd_favicon.as_str()).filter(|s| !s.is_empty()),
            None,
        );

        // Build status response packet
        let mut body = BytesMut::new();
        packet::write_byte(&mut body, 0x00); // Client bound: Status Response
        packet::write_string(&mut body, std::str::from_utf8(&motd).unwrap_or("{}"));

        let mut packet_buf = BytesMut::new();
        let mut tmp = [0u8; 5];
        let n = VarInt(body.len() as i32).encode(&mut tmp)?;
        packet_buf.extend_from_slice(&tmp[..n]);
        packet_buf.extend_from_slice(&body);

        stream.write_all(&packet_buf).await?;

        match config.ping_mode.as_str() {
            "disconnect" => {
                // Close immediately
            }
            "0ms" => {
                // Send pong with 0 latency
                let mut pong = BytesMut::new();
                packet::write_byte(&mut pong, 0x01); // Ping Response
                packet::write_u64_be(&mut pong, 0); // 0ms timestamp
                let mut pkt = BytesMut::new();
                let n = VarInt(pong.len() as i32).encode(&mut tmp)?;
                pkt.extend_from_slice(&tmp[..n]);
                pkt.extend_from_slice(&pong);
                stream.write_all(&pkt).await?;
            }
            _ => {
                // Default: read ping request and echo back
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

    /// Handle a LOGIN request.
    async fn handle_login(
        &self,
        mut stream: Stream,
        metadata: &ConnectionMetadata,
    ) -> Result<(), Error> {
        let mc = metadata.minecraft.as_ref().unwrap();
        let config = self.config.read().await;

        // Name access control
        if !config.name_access.mode.is_empty() {
            let mode = AccessMode::from_str(&config.name_access.mode);
            if mode != AccessMode::Default {
                let name = if config.name_access.lower_case {
                    mc.player_name.to_lowercase()
                } else {
                    mc.player_name.clone()
                };
                let lists = &self.name_access_lists;
                if !common::access::check(lists, mode, &name) {
                    drop(config);
                    self.send_kick(
                        stream,
                        &generate_kick_message(&self.name, &mc.player_name),
                        "name access control",
                    )
                    .await?;
                    return Ok(());
                }
            }
        }

        // Player count limit
        if config.online_count.enable_max_limit
            && config.online_count.max <= self.online_count.load(Ordering::Relaxed)
        {
            drop(config);
            self.send_kick(
                stream,
                &generate_player_number_limit_exceeded_message(&self.name, &mc.player_name),
                "player number limiter",
            )
            .await?;
            return Ok(());
        }

        // Determine target hostname and port
        let hostname = if config.enable_hostname_rewrite {
            if config.rewritten_hostname.is_empty() {
                self.target_address.clone()
            } else {
                config.rewritten_hostname.clone()
            }
        } else if mc.rewritten_destination.is_empty() {
            mc.clean_origin_destination().to_string()
        } else {
            mc.rewritten_destination.clone()
        };

        let hostname = if !config.ignore_fml_suffix && mc.is_fml() {
            format!("{}\x00{}", hostname, mc.fml_markup())
        } else {
            hostname
        };

        let port = if mc.rewritten_port > 0 {
            mc.rewritten_port
        } else if self.target_port > 0 {
            self.target_port
        } else {
            mc.origin_port
        };

        let target_addr = format!("{}:{}", self.target_address, self.target_port);
        drop(config);

        // Connect to backend
        log::debug!("[{}] Resolving backend: {}", self.name, target_addr);
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
                    "[{}] Failed to connect to backend {} for player {}: {e}",
                    self.name,
                    target_addr,
                    mc.player_name
                );
                let msg = protocol::message::Message {
                    color: "#FF5555".into(),
                    text: format!(
                        "§cUnable to connect to backend server.\n§7{}",
                        e
                    ),
                    ..protocol::message::Message::default()
                };
                return self.send_kick(stream, &msg, "backend unreachable").await;
            }
        };

        // Build rewritten handshake packet
        let mut handshake = BytesMut::new();
        packet::write_byte(&mut handshake, 0x00); // Server bound: Handshake
        let mut tmp = [0u8; 5];
        let n = VarInt(mc.protocol_version).encode(&mut tmp)?;
        handshake.extend_from_slice(&tmp[..n]);
        packet::write_string(&mut handshake, &hostname);
        packet::write_u16_be(&mut handshake, port);
        packet::write_byte(&mut handshake, 2); // intent = login

        let mut packet_buf = BytesMut::new();
        let n = VarInt(handshake.len() as i32).encode(&mut tmp)?;
        packet_buf.extend_from_slice(&tmp[..n]);
        packet_buf.extend_from_slice(&handshake);

        // Append the original post-handshake bytes (login start packet)
        if let Some(post) = stream.post_handshake_bytes(mc.sniff_position) {
            packet_buf.extend_from_slice(post);
        } else {
            // Fallback: reconstruct login start from metadata
            let mut login_start = BytesMut::new();
            packet::write_byte(&mut login_start, 0x00); // Login Start
            packet::write_string(&mut login_start, &mc.player_name);
            if mc.protocol_version >= 764 && mc.uuid != [0u8; 16] {
                login_start.extend_from_slice(&mc.uuid);
            }
            let n = VarInt(login_start.len() as i32).encode(&mut tmp)?;
            packet_buf.extend_from_slice(&tmp[..n]);
            packet_buf.extend_from_slice(&login_start);
        }

        target.write_all(&packet_buf).await?;

        // Consume ALL peeked bytes (handshake + login start) so they are
        // not replayed to the backend during relay.
        stream.consume_peek(usize::MAX);

        log::info!(
            "[{}] Created Minecraft connection for player {}",
            self.name,
            mc.player_name
        );

        self.online_count.fetch_add(1, Ordering::Relaxed);
        let result = relay::relay(stream, target, crate::relay::RelayConfig::default()).await;
        self.online_count.fetch_sub(1, Ordering::Relaxed);

        match &result {
            Ok(()) => log::info!(
                "[{}] Closed Minecraft connection for player {}",
                self.name,
                mc.player_name
            ),
            Err(e) => log::warn!(
                "[{}] Minecraft connection error for player {}: {e}",
                self.name,
                mc.player_name
            ),
        }

        result
    }

    /// Send a kick message and close the connection.
    async fn send_kick(
        &self,
        mut stream: Stream,
        message: &protocol::message::Message,
        reason: &str,
    ) -> Result<(), Error> {
        let json = message.to_json()?;
        let mut body = BytesMut::new();
        packet::write_byte(&mut body, 0x00); // Client bound: Disconnect (login)
        packet::write_string(&mut body, &json);

        let mut packet_buf = BytesMut::new();
        let mut tmp = [0u8; 5];
        let n = VarInt(body.len() as i32).encode(&mut tmp)?;
        packet_buf.extend_from_slice(&tmp[..n]);
        packet_buf.extend_from_slice(&body);

        stream.write_all(&packet_buf).await?;

        log::warn!("[{}] Kicked player: {}", self.name, reason);
        Ok(())
    }
}

#[async_trait::async_trait]
impl Outbound for MinecraftOutbound {
    fn name(&self) -> &str {
        &self.name
    }

    async fn handle_connection(
        &self,
        conn: Stream,
        metadata: ConnectionMetadata,
    ) -> Result<(), Error> {
        let mc = metadata
            .minecraft
            .as_ref()
            .ok_or_else(|| Error::Protocol("Minecraft metadata required".into()))?;

        if !mc.valid() {
            return Err(Error::Protocol("invalid Minecraft protocol".into()));
        }

        match mc.next_state {
            1 => self.handle_status(conn, &metadata).await,
            2 => self.handle_login(conn, &metadata).await,
            3 => {
                log::debug!("Minecraft transfer not yet implemented");
                Ok(())
            }
            other => Err(Error::Protocol(format!("unknown intent: {other}"))),
        }
    }
}
