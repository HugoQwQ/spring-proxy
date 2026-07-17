//! Top-level proxy orchestrator.
//!
//! **Interface:** [`Runner::run`] — initializes outbounds → router → services
//! and runs the proxy until a shutdown signal.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use common::Error;

use crate::config::{OutboundConfig, Root};
use crate::outbound::{MinecraftOutbound, Outbound, PlainOutbound};
use crate::route::Router;
use crate::service::Service;

/// The running proxy orchestrator.
pub struct Runner {
    config: Root,
}

impl Runner {
    /// Create a new runner from a config root.
    pub fn new(config: Root) -> Self {
        Self { config }
    }

    /// Run the proxy server.
    ///
    /// Initializes outbounds, router, and services, then runs until
    /// a shutdown signal (Ctrl+C).
    pub async fn run(&self) -> Result<(), Error> {
        log::info!("Spring Proxy v{}", env!("CARGO_PKG_VERSION"));

        // Initialize outbounds
        let mut outbound_map: HashMap<String, Arc<dyn Outbound>> = HashMap::new();
        for outbound_config in &self.config.outbounds {
            let outbound = build_outbound(outbound_config)?;
            outbound_map.insert(outbound_config.name.clone(), outbound);
        }
        log::info!("Initialized {} outbounds", outbound_map.len());

        // Initialize router
        let router = Arc::new(Router::new(&self.config.router, outbound_map)?);
        log::info!(
            "Initialized router with {} rules",
            self.config.router.rules.len()
        );

        // Initialize and start services
        let mut service_handles = Vec::new();
        for service_config in &self.config.services {
            let legacy_outbound = if !service_config.outbound.is_empty() {
                router.find_outbound(&service_config.outbound)
            } else {
                None
            };

            let mut service = Service::new(service_config.clone(), router.clone(), legacy_outbound);
            service.load_access_lists().await?;

            let service_name = service_config.name.clone();
            let handle = smol::spawn(async move {
                if let Err(e) = service.start().await {
                    log::error!("Service '{}' error: {e}", service_name);
                }
            });
            service_handles.push(handle);
        }

        log::info!(
            "Started {} services. Press Ctrl+C to stop.",
            service_handles.len()
        );

        // Wait for shutdown signal
        let (shutdown_tx, shutdown_rx) = async_channel::bounded::<()>(1);
        let sig_tx = shutdown_tx.clone();
        ctrlc::set_handler(move || {
            let _ = sig_tx.try_send(());
        })
        .map_err(|e| Error::Internal(format!("failed to set Ctrl+C handler: {e}")))?;

        shutdown_rx.recv().await.ok();
        log::info!("Shutdown signal received");

        Ok(())
    }
}

/// Build an outbound from its configuration.
fn build_outbound(config: &OutboundConfig) -> Result<Arc<dyn Outbound>, Error> {
    if config.minecraft.is_some() {
        let mc_config = config.minecraft.clone().unwrap();
        Ok(Arc::new(MinecraftOutbound::new(
            &config.name,
            &config.target_address,
            config.target_port,
            mc_config,
        )))
    } else {
        Ok(Arc::new(PlainOutbound::new(
            &config.name,
            &config.target_address,
            config.target_port,
        )))
    }
}

// Legacy simple config for backward compatibility

/// Simple TOML-based configuration (legacy).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Config {
    #[serde(default = "default_listen")]
    pub listen: SocketAddr,
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
    #[serde(default)]
    pub relay: crate::relay::RelayConfig,
    #[serde(default)]
    pub max_connections: usize,
    #[serde(default = "default_handshake_timeout")]
    pub handshake_timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            routes: Vec::new(),
            relay: crate::relay::RelayConfig::default(),
            max_connections: 0,
            handshake_timeout_secs: default_handshake_timeout(),
        }
    }
}

fn default_listen() -> SocketAddr {
    "0.0.0.0:25565"
        .parse()
        .expect("invalid default listen address")
}

fn default_handshake_timeout() -> u64 {
    5
}

/// A single route in the legacy config file.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct RouteConfig {
    #[serde(default = "default_match")]
    pub match_on: String,
    pub target: SocketAddr,
    pub label: Option<String>,
}

fn default_match() -> String {
    "any".into()
}

// Tests

#[cfg(test)]
mod tests {
    #[test]
    fn parse_match_variants() {
        use crate::router;
        assert!(matches!(parse_match("any"), router::Match::Any));
        assert!(matches!(
            parse_match("host:hypixel.net"),
            router::Match::Host(ref h) if h == "hypixel.net"
        ));
    }

    fn parse_match(s: &str) -> crate::router::Match {
        let s = s.trim();
        if let Some((key, value)) = s.split_once(':') {
            match key {
                "host" => crate::router::Match::Host(value.to_string()),
                "pattern" => crate::router::Match::Pattern(value.to_string()),
                "proto" => value
                    .parse::<i32>()
                    .map(crate::router::Match::ProtocolVersion)
                    .unwrap_or(crate::router::Match::Any),
                "port" => value
                    .parse::<u16>()
                    .map(crate::router::Match::Port)
                    .unwrap_or(crate::router::Match::Any),
                "state" => match value {
                    "login" => crate::router::Match::NextState(protocol::NextState::Login),
                    "status" => crate::router::Match::NextState(protocol::NextState::Status),
                    _ => crate::router::Match::Any,
                },
                _ => crate::router::Match::Any,
            }
        } else if s == "any" {
            crate::router::Match::Any
        } else {
            crate::router::Match::Host(s.to_string())
        }
    }

    #[test]
    fn bare_string_is_host_match() {
        use crate::router;
        assert!(matches!(
            parse_match("hypixel.net"),
            router::Match::Host(ref h) if h == "hypixel.net"
        ));
    }
}
