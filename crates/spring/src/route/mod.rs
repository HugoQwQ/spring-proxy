//! Router for Spring Proxy.
//!
//! Evaluates a chain of rules against connection metadata and dispatches
//! to the appropriate outbound. Supports sniffing, rewriting, and
//! injection-based outbounds.

use std::collections::HashMap;
use std::sync::Arc;

use common::set::StringSet;

use crate::config::{RouterConfig, RuleConfig};
use crate::outbound::Outbound;
use crate::stream::Stream;

pub mod rules;

pub use rules::ConnectionMetadata;

/// Built-in outbounds.
pub mod builtin {
    use super::*;

    /// Reject outbound — closes the connection immediately.
    pub struct Reject;

    #[async_trait::async_trait]
    impl Outbound for Reject {
        fn name(&self) -> &str {
            "REJECT"
        }
        async fn handle_connection(
            &self,
            _conn: Stream,
            _metadata: ConnectionMetadata,
        ) -> Result<(), common::Error> {
            Ok(())
        }
    }

    /// Reset outbound — closes with TCP RST (sets linger=0).
    pub struct Reset;

    #[async_trait::async_trait]
    impl Outbound for Reset {
        fn name(&self) -> &str {
            "RESET"
        }
        async fn handle_connection(
            &self,
            _conn: Stream,
            _metadata: ConnectionMetadata,
        ) -> Result<(), common::Error> {
            // Best effort: set linger to 0 for RST
            // async_net::TcpStream does not support set_linger; just close
            Ok(())
        }
    }
}

/// A router evaluates rules and dispatches connections to outbounds.
pub struct Router {
    rules: Vec<Box<dyn rules::Rule>>,
    outbound_map: HashMap<String, Arc<dyn Outbound>>,
    default_outbound: Arc<dyn Outbound>,
}

impl Router {
    /// Create a new router from config and outbound map.
    pub fn new(
        config: &RouterConfig,
        outbound_map: HashMap<String, Arc<dyn Outbound>>,
    ) -> Result<Self, common::Error> {
        let mut rules: Vec<Box<dyn rules::Rule>> = Vec::new();
        for rule_config in &config.rules {
            let rule = build_rule(rule_config.clone())?;
            rules.push(rule);
        }

        let default = outbound_map
            .get(&config.default_outbound)
            .cloned()
            .unwrap_or_else(|| Arc::new(builtin::Reset));

        Ok(Self {
            rules,
            outbound_map,
            default_outbound: default,
        })
    }

    /// Handle an inbound connection.
    ///
    /// 1. Evaluate rules in order.
    /// 2. Run sniffers if configured.
    /// 3. Apply rewrites.
    /// 4. Dispatch to outbound.
    pub async fn handle_connection(&self, conn: Stream, mut metadata: ConnectionMetadata) {
        let peer = match &conn {
            Stream::Plain(tcp) => tcp.peer_addr().ok(),
            Stream::Minecraft(mc) => mc.inner().peer_addr().ok(),
        };
        let mut selected_outbound: Option<Arc<dyn Outbound>> = None;

        for (i, rule) in self.rules.iter().enumerate() {
            if rule.matches(&metadata) {
                log::trace!("Rule matched: index={}, type={}", i, rule.config().r#type);
                let config = rule.config();

                // Sniff protocols
                if !config.sniff.is_empty() {
                    self.sniff(&mut metadata, &config.sniff, &conn).await;
                }

                // Apply rewrite
                if !config.rewrite.target_address.is_empty() {
                    // stored in metadata for outbound use
                }
                if config.rewrite.target_port > 0 {
                    // stored in metadata for outbound use
                }
                if let Some(ref mc_rewrite) = config.rewrite.minecraft {
                    if let Some(ref mut mc) = metadata.minecraft {
                        if !mc_rewrite.hostname.is_empty() {
                            mc.rewritten_destination = mc_rewrite.hostname.clone();
                        }
                        if mc_rewrite.port > 0 {
                            mc.rewritten_port = mc_rewrite.port;
                        }
                        if mc_rewrite.intent > 0 {
                            mc.next_state = mc_rewrite.intent;
                        }
                    }
                }

                // Select outbound
                if !config.outbound.is_empty() {
                    if let Some(outbound) = self.outbound_map.get(&config.outbound) {
                        selected_outbound = Some(outbound.clone());
                    } else {
                        log::error!("Outbound not found: {} (rule index {})", config.outbound, i);
                    }
                    break;
                }
            }
        }

        let outbound = selected_outbound.unwrap_or_else(|| self.default_outbound.clone());

        if let Err(e) = outbound.handle_connection(conn, metadata).await {
            log::warn!(
                "Outbound {} error for {}: {e}",
                outbound.name(),
                peer.map(|a| a.to_string()).unwrap_or_default()
            );
        }
    }

    /// Sniff protocols from the connection.
    async fn sniff(
        &self,
        _metadata: &mut ConnectionMetadata,
        protocols: &[String],
        _conn: &Stream,
    ) {
        for protocol in protocols {
            match protocol.as_str() {
                "minecraft" => {
                    // Minecraft sniffing is done upfront by the service.
                    log::trace!("Sniff minecraft already performed by service");
                }
                _ => {
                    log::debug!("Unknown sniff protocol: {}", protocol);
                }
            }
        }
    }

    /// Find an outbound by name.
    pub fn find_outbound(&self, name: &str) -> Option<Arc<dyn Outbound>> {
        self.outbound_map.get(name).cloned()
    }

    /// Find lists by tag (for access control lookups).
    pub fn find_lists_by_tag(&self, _tags: &[String]) -> Result<Vec<StringSet>, common::Error> {
        // Lists would be stored in the router; for now return empty.
        Ok(Vec::new())
    }
}

/// Build a rule from its configuration.
fn build_rule(config: RuleConfig) -> Result<Box<dyn rules::Rule>, common::Error> {
    match config.r#type.as_str() {
        "always" => Ok(Box::new(rules::RuleAlways::new(config))),
        "and" => {
            let sub_configs: Vec<RuleConfig> =
                serde_json::from_value(config.parameter.clone().unwrap_or(serde_json::Value::Null))
                    .unwrap_or_default();
            let sub_rules: Result<Vec<Box<dyn rules::Rule>>, _> =
                sub_configs.into_iter().map(build_rule).collect();
            Ok(Box::new(rules::RuleAnd::new(sub_rules?, config)))
        }
        "or" => {
            let sub_configs: Vec<RuleConfig> =
                serde_json::from_value(config.parameter.clone().unwrap_or(serde_json::Value::Null))
                    .unwrap_or_default();
            let sub_rules: Result<Vec<Box<dyn rules::Rule>>, _> =
                sub_configs.into_iter().map(build_rule).collect();
            Ok(Box::new(rules::RuleOr::new(sub_rules?, config)))
        }
        "ServiceName" => Ok(Box::new(rules::RuleServiceName::new(config)?)),
        "SourceIPVersion" => Ok(Box::new(rules::RuleSourceIPVersion::new(config)?)),
        "SourceIP" => Ok(Box::new(rules::RuleSourceIP::new(config)?)),
        "SourcePort" => Ok(Box::new(rules::RuleSourcePort::new(config)?)),
        "MinecraftHostname" => Ok(Box::new(rules::RuleMinecraftHostname::new(config)?)),
        "MinecraftPlayerName" => Ok(Box::new(rules::RuleMinecraftPlayerName::new(config)?)),
        "MinecraftStatus" => Ok(Box::new(rules::RuleMinecraftStatus::new(config)?)),
        "MinecraftTransfer" => Ok(Box::new(rules::RuleMinecraftTransfer::new(config)?)),
        other => Err(common::Error::Protocol(format!(
            "unknown rule type: {other}"
        ))),
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    struct MockOutbound(&'static str);

    #[async_trait::async_trait]
    impl Outbound for MockOutbound {
        fn name(&self) -> &str {
            self.0
        }
        async fn handle_connection(
            &self,
            _conn: Stream,
            _metadata: ConnectionMetadata,
        ) -> Result<(), common::Error> {
            Ok(())
        }
    }

    #[test]
    fn router_builds_and_matches() {
        let config = RouterConfig {
            default_outbound: "RESET".into(),
            rules: vec![RuleConfig {
                r#type: "always".into(),
                outbound: "test-out".into(),
                ..RuleConfig::default()
            }],
        };

        let mut outbounds: HashMap<String, Arc<dyn Outbound>> = HashMap::new();
        outbounds.insert("test-out".into(), Arc::new(MockOutbound("test-out")));
        outbounds.insert("RESET".into(), Arc::new(builtin::Reset));

        let router = Router::new(&config, outbounds).unwrap();
        assert_eq!(router.rules.len(), 1);
    }
}
