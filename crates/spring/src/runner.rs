//! Top-level proxy orchestrator.
//!
//! **Interface:** [`Runner::run`] — initializes outbounds → router → services
//! and runs the proxy until a shutdown signal.

use std::collections::HashMap;
use std::path::Path;
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

    /// Load a runner from a config file.
    ///
    /// If the file does not exist, generates a default config, writes it,
    /// and continues with that default.
    pub fn from_config_file(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let config: Root = match std::fs::read_to_string(path) {
            Ok(content) => toml::from_str(&content)
                .map_err(|e| Error::Internal(format!("failed to parse config: {e}")))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::warn!("Config file {:?} not found, generating default", path);
                let default = Root::generate_default();
                let toml_str = toml::to_string_pretty(&default)
                    .map_err(|e| Error::Internal(format!("failed to serialize config: {e}")))?;
                match std::fs::write(path, &toml_str) {
                    Ok(_) => log::info!("Created default config at {:?}", path),
                    Err(e) => log::warn!("Could not create default config: {e}"),
                }
                default
            }
            Err(e) => {
                return Err(Error::Internal(format!(
                    "failed to read config {:?}: {e}",
                    path
                )));
            }
        };
        log::info!("Config loaded: {:?}", path);
        Ok(Self::new(config))
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
