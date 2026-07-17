//! Routing rules for Spring Proxy.
//!
//! Implements ZBProxy's rule types:
//! - `always` — matches every connection
//! - `and` / `or` — logical composition
//! - `ServiceName` — match by inbound service name
//! - `SourceIPVersion` — match by IP version (4 or 6)
//! - `SourceIP` — match by source IP/CIDR
//! - `SourcePort` — match by source port
//! - `MinecraftHostname` — match by sniffed Minecraft hostname
//! - `MinecraftPlayerName` — match by sniffed Minecraft player name
//! - `MinecraftStatus` — match if next state is status
//! - `MinecraftTransfer` — match if next state is transfer

use std::collections::HashSet;

use common::domain::Matcher;

use crate::config::RuleConfig;

/// A routing rule that can match connection metadata.
pub trait Rule: Send + Sync {
    /// Returns the config for this rule.
    fn config(&self) -> &RuleConfig;
    /// Check if this rule matches the given metadata.
    fn matches(&self, metadata: &ConnectionMetadata) -> bool;
}

/// Metadata about an inbound connection, used for rule matching.
#[derive(Debug, Clone)]
pub struct ConnectionMetadata {
    /// Service name that accepted the connection.
    pub service_name: String,
    /// Source address.
    pub source_addr: std::net::SocketAddr,
    /// Sniffed Minecraft metadata, if any.
    pub minecraft: Option<protocol::MinecraftMetadata>,
}

impl Default for ConnectionMetadata {
    fn default() -> Self {
        Self {
            service_name: String::new(),
            source_addr: std::net::SocketAddr::from(([0, 0, 0, 0], 0)),
            minecraft: None,
        }
    }
}

// Always rule

pub struct RuleAlways {
    config: RuleConfig,
}

impl RuleAlways {
    pub fn new(config: RuleConfig) -> Self {
        Self { config }
    }
}

impl Rule for RuleAlways {
    fn config(&self) -> &RuleConfig {
        &self.config
    }
    fn matches(&self, _metadata: &ConnectionMetadata) -> bool {
        !self.config.invert
    }
}

// Logical AND rule

pub struct RuleAnd {
    rules: Vec<Box<dyn Rule>>,
    config: RuleConfig,
}

impl RuleAnd {
    pub fn new(rules: Vec<Box<dyn Rule>>, config: RuleConfig) -> Self {
        Self { rules, config }
    }
}

impl Rule for RuleAnd {
    fn config(&self) -> &RuleConfig {
        &self.config
    }
    fn matches(&self, metadata: &ConnectionMetadata) -> bool {
        let mut result = true;
        for rule in &self.rules {
            if !rule.matches(metadata) {
                result = false;
                break;
            }
        }
        if self.config.invert {
            result = !result;
        }
        result
    }
}

// Logical OR rule

pub struct RuleOr {
    rules: Vec<Box<dyn Rule>>,
    config: RuleConfig,
}

impl RuleOr {
    pub fn new(rules: Vec<Box<dyn Rule>>, config: RuleConfig) -> Self {
        Self { rules, config }
    }
}

impl Rule for RuleOr {
    fn config(&self) -> &RuleConfig {
        &self.config
    }
    fn matches(&self, metadata: &ConnectionMetadata) -> bool {
        let mut result = false;
        for rule in &self.rules {
            if rule.matches(metadata) {
                result = true;
                break;
            }
        }
        if self.config.invert {
            result = !result;
        }
        result
    }
}

// Service name rule

pub struct RuleServiceName {
    names: HashSet<String>,
    config: RuleConfig,
}

impl RuleServiceName {
    pub fn new(config: RuleConfig) -> Result<Self, common::Error> {
        let value = config.parameter.clone().unwrap_or(serde_json::Value::Null);
        let names: Vec<String> = if let Ok(single) = serde_json::from_value::<String>(value.clone())
        {
            vec![single]
        } else {
            serde_json::from_value(value).unwrap_or_default()
        };
        let set: HashSet<String> = names.into_iter().collect();
        Ok(Self { names: set, config })
    }
}

impl Rule for RuleServiceName {
    fn config(&self) -> &RuleConfig {
        &self.config
    }
    fn matches(&self, metadata: &ConnectionMetadata) -> bool {
        let mut result = self.names.contains(&metadata.service_name);
        if self.config.invert {
            result = !result;
        }
        result
    }
}

// Source IP version rule

pub struct RuleSourceIPVersion {
    version: u8,
    config: RuleConfig,
}

impl RuleSourceIPVersion {
    pub fn new(config: RuleConfig) -> Result<Self, common::Error> {
        let version: u8 =
            serde_json::from_value(config.parameter.clone().unwrap_or(serde_json::Value::Null))
                .map_err(|e| common::Error::Protocol(format!("bad IP version: {e}")))?;
        Ok(Self { version, config })
    }
}

impl Rule for RuleSourceIPVersion {
    fn config(&self) -> &RuleConfig {
        &self.config
    }
    fn matches(&self, metadata: &ConnectionMetadata) -> bool {
        let is_v4 = metadata.source_addr.is_ipv4();
        let mut result = if is_v4 {
            self.version == 4
        } else {
            self.version == 6
        };
        if self.config.invert {
            result = !result;
        }
        result
    }
}

// Source IP rule

pub struct RuleSourceIP {
    cidrs: Vec<ipnet::IpNet>,
    config: RuleConfig,
}

impl RuleSourceIP {
    pub fn new(config: RuleConfig) -> Result<Self, common::Error> {
        let cidrs: Vec<String> =
            serde_json::from_value(config.parameter.clone().unwrap_or(serde_json::Value::Null))
                .unwrap_or_default();
        let nets: Result<Vec<ipnet::IpNet>, _> = cidrs.iter().map(|s| s.parse()).collect();
        let nets = nets.map_err(|e| common::Error::Protocol(format!("bad CIDR: {e}")))?;
        Ok(Self {
            cidrs: nets,
            config,
        })
    }
}

impl Rule for RuleSourceIP {
    fn config(&self) -> &RuleConfig {
        &self.config
    }
    fn matches(&self, metadata: &ConnectionMetadata) -> bool {
        let ip = metadata.source_addr.ip();
        let mut result = self.cidrs.iter().any(|net| net.contains(&ip));
        if self.config.invert {
            result = !result;
        }
        result
    }
}

// Source port rule

pub struct RuleSourcePort {
    ports: HashSet<u16>,
    config: RuleConfig,
}

impl RuleSourcePort {
    pub fn new(config: RuleConfig) -> Result<Self, common::Error> {
        let ports: Vec<u16> =
            serde_json::from_value(config.parameter.clone().unwrap_or(serde_json::Value::Null))
                .unwrap_or_default();
        let set: HashSet<u16> = ports.into_iter().collect();
        Ok(Self { ports: set, config })
    }
}

impl Rule for RuleSourcePort {
    fn config(&self) -> &RuleConfig {
        &self.config
    }
    fn matches(&self, metadata: &ConnectionMetadata) -> bool {
        let mut result = self.ports.contains(&metadata.source_addr.port());
        if self.config.invert {
            result = !result;
        }
        result
    }
}

// Minecraft hostname rule

pub struct RuleMinecraftHostname {
    matcher: Matcher,
    config: RuleConfig,
}

impl RuleMinecraftHostname {
    pub fn new(config: RuleConfig) -> Result<Self, common::Error> {
        let param: Vec<String> =
            serde_json::from_value(config.parameter.clone().unwrap_or(serde_json::Value::Null))
                .unwrap_or_default();
        Ok(Self {
            matcher: Matcher::new(&param, &[]),
            config,
        })
    }
}

impl Rule for RuleMinecraftHostname {
    fn config(&self) -> &RuleConfig {
        &self.config
    }
    fn matches(&self, metadata: &ConnectionMetadata) -> bool {
        let mut result = metadata
            .minecraft
            .as_ref()
            .map(|m| self.matcher.matches(m.clean_origin_destination()))
            .unwrap_or(false);
        if self.config.invert {
            result = !result;
        }
        result
    }
}

// Minecraft player name rule

pub struct RuleMinecraftPlayerName {
    names: HashSet<String>,
    lower_case: bool,
    config: RuleConfig,
}

impl RuleMinecraftPlayerName {
    pub fn new(config: RuleConfig) -> Result<Self, common::Error> {
        let param: Vec<String> =
            serde_json::from_value(config.parameter.clone().unwrap_or(serde_json::Value::Null))
                .unwrap_or_default();
        let set: HashSet<String> = param.into_iter().collect();
        Ok(Self {
            names: set,
            lower_case: false,
            config,
        })
    }
}

impl Rule for RuleMinecraftPlayerName {
    fn config(&self) -> &RuleConfig {
        &self.config
    }
    fn matches(&self, metadata: &ConnectionMetadata) -> bool {
        let mut result = metadata
            .minecraft
            .as_ref()
            .map(|m| {
                let name = if self.lower_case {
                    m.player_name.to_lowercase()
                } else {
                    m.player_name.clone()
                };
                self.names.contains(&name)
            })
            .unwrap_or(false);
        if self.config.invert {
            result = !result;
        }
        result
    }
}

// Minecraft status rule

pub struct RuleMinecraftStatus {
    config: RuleConfig,
}

impl RuleMinecraftStatus {
    pub fn new(config: RuleConfig) -> Result<Self, common::Error> {
        Ok(Self { config })
    }
}

impl Rule for RuleMinecraftStatus {
    fn config(&self) -> &RuleConfig {
        &self.config
    }
    fn matches(&self, metadata: &ConnectionMetadata) -> bool {
        let mut result = metadata
            .minecraft
            .as_ref()
            .map(|m| m.next_state == 1)
            .unwrap_or(false);
        if self.config.invert {
            result = !result;
        }
        result
    }
}

// Minecraft transfer rule

pub struct RuleMinecraftTransfer {
    config: RuleConfig,
}

impl RuleMinecraftTransfer {
    pub fn new(config: RuleConfig) -> Result<Self, common::Error> {
        Ok(Self { config })
    }
}

impl Rule for RuleMinecraftTransfer {
    fn config(&self) -> &RuleConfig {
        &self.config
    }
    fn matches(&self, metadata: &ConnectionMetadata) -> bool {
        let mut result = metadata
            .minecraft
            .as_ref()
            .map(|m| m.next_state == 3)
            .unwrap_or(false);
        if self.config.invert {
            result = !result;
        }
        result
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn make_meta() -> ConnectionMetadata {
        ConnectionMetadata {
            service_name: "test-service".into(),
            source_addr: SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 12345).into(),
            ..ConnectionMetadata::default()
        }
    }

    #[test]
    fn rule_always() {
        let rule = RuleAlways::new(RuleConfig::default());
        assert!(rule.matches(&make_meta()));
    }

    #[test]
    fn rule_always_inverted() {
        let mut config = RuleConfig::default();
        config.invert = true;
        let rule = RuleAlways::new(config);
        assert!(!rule.matches(&make_meta()));
    }

    #[test]
    fn rule_service_name() {
        let config = RuleConfig {
            parameter: Some(serde_json::json!("test-service")),
            ..RuleConfig::default()
        };
        let rule = RuleServiceName::new(config).unwrap();
        assert!(rule.matches(&make_meta()));
    }

    #[test]
    fn rule_source_ip_version() {
        let config = RuleConfig {
            parameter: Some(serde_json::json!(4)),
            ..RuleConfig::default()
        };
        let rule = RuleSourceIPVersion::new(config).unwrap();
        assert!(rule.matches(&make_meta()));
    }

    #[test]
    fn rule_source_port() {
        let config = RuleConfig {
            parameter: Some(serde_json::json!([12345])),
            ..RuleConfig::default()
        };
        let rule = RuleSourcePort::new(config).unwrap();
        assert!(rule.matches(&make_meta()));
    }

    #[test]
    fn rule_minecraft_status() {
        let mut meta = make_meta();
        let mut mc = protocol::MinecraftMetadata::default();
        mc.next_state = 1;
        meta.minecraft = Some(mc);

        let rule = RuleMinecraftStatus::new(RuleConfig::default()).unwrap();
        assert!(rule.matches(&meta));
    }
}
