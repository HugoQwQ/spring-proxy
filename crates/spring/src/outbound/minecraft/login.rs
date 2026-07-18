//! Minecraft login handler.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use async_lock::RwLock;
use common::{Error, access::AccessMode};
use protocol::kick::{generate_kick_message, generate_player_number_limit_exceeded_message};
use smol::io::AsyncWriteExt;

use crate::config::MinecraftOutboundConfig;
use crate::outbound::minecraft::packets;
use crate::relay;
use crate::route::ConnectionMetadata;
use crate::stream::Stream;

/// Handles Minecraft login (join server) requests.
pub(crate) struct LoginHandler {
    name: String,
    target_address: String,
    target_port: u16,
    config: Arc<RwLock<MinecraftOutboundConfig>>,
    online_count: Arc<AtomicI32>,
    name_access_lists: Vec<common::set::StringSet>,
}

impl LoginHandler {
    pub(crate) fn new(
        name: impl Into<String>,
        target_address: impl Into<String>,
        target_port: u16,
        config: Arc<RwLock<MinecraftOutboundConfig>>,
        online_count: Arc<AtomicI32>,
        name_access_lists: Vec<common::set::StringSet>,
    ) -> Self {
        Self {
            name: name.into(),
            target_address: target_address.into(),
            target_port,
            config,
            online_count,
            name_access_lists,
        }
    }

    /// Handle a LOGIN request.
    pub(crate) async fn handle(
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
                if !common::access::check(&self.name_access_lists, mode, &name) {
                    drop(config);
                    return super::send_kick(
                        &mut stream,
                        &generate_kick_message(&self.name, &mc.player_name),
                    )
                    .await;
                }
            }
        }

        // Player count limit
        if config.online_count.enable_max_limit
            && config.online_count.max <= self.online_count.load(Ordering::Relaxed)
        {
            drop(config);
            return super::send_kick(
                &mut stream,
                &generate_player_number_limit_exceeded_message(&self.name, &mc.player_name),
            )
            .await;
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
                    text: format!("§cUnable to connect to backend server.\n§7{e}"),
                    ..protocol::message::Message::default()
                };
                return super::send_kick(&mut stream, &msg).await;
            }
        };

        // Build rewritten handshake + login start
        let mut packet_buf =
            packets::build_handshake_packet(mc.protocol_version, &hostname, port, 2)?;

        if let Some(post) = stream.post_handshake_bytes(mc.sniff_position) {
            packet_buf.extend_from_slice(post);
        } else {
            let login_pkt =
                packets::build_login_start_packet(&mc.player_name, &mc.uuid, mc.protocol_version)?;
            packet_buf.extend_from_slice(&login_pkt);
        }

        target.write_all(&packet_buf).await?;
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
}
