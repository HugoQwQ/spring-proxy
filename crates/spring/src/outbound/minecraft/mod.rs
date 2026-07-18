//! Minecraft outbound proxy.

use std::sync::Arc;
use std::sync::atomic::AtomicI32;

use async_lock::RwLock;
use common::Error;
use protocol::motd::DEFAULT_MOTD_FAVICON;
use smol::io::AsyncWriteExt;

use crate::config::MinecraftOutboundConfig;
use crate::outbound::Outbound;
use crate::route::ConnectionMetadata;
use crate::stream::Stream;

mod login;
mod packets;
mod status;

use login::LoginHandler;
use status::StatusHandler;

/// Send a kick message and close the connection.
async fn send_kick(stream: &mut Stream, message: &protocol::message::Message) -> Result<(), Error> {
    let json = message.to_json()?;
    let packet_buf = packets::build_kick_packet(&json)?;
    stream.write_all(&packet_buf).await?;
    Ok(())
}

/// A Minecraft-specific outbound.
///
/// Thin orchestrator: holds shared state and dispatches to
/// `StatusHandler` or `LoginHandler` based on the connection's next state.
pub struct MinecraftOutbound {
    name: String,
    target_address: String,
    target_port: u16,
    config: Arc<RwLock<MinecraftOutboundConfig>>,
    hostname_access_lists: Vec<common::set::StringSet>,
    name_access_lists: Vec<common::set::StringSet>,
    online_count: Arc<AtomicI32>,
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
            config: Arc::new(RwLock::new(config)),
            hostname_access_lists: Vec::new(),
            name_access_lists: Vec::new(),
            online_count: Arc::new(AtomicI32::new(0)),
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
            1 => {
                let handler = StatusHandler::new(
                    &self.name,
                    &self.target_address,
                    self.target_port,
                    self.config.clone(),
                    self.online_count.clone(),
                );
                handler.handle(conn, &metadata).await
            }
            2 => {
                let handler = LoginHandler::new(
                    &self.name,
                    &self.target_address,
                    self.target_port,
                    self.config.clone(),
                    self.online_count.clone(),
                    self.name_access_lists.clone(),
                );
                handler.handle(conn, &metadata).await
            }
            3 => {
                log::debug!("Minecraft transfer not yet implemented");
                Ok(())
            }
            other => Err(Error::Protocol(format!("unknown intent: {other}"))),
        }
    }
}
