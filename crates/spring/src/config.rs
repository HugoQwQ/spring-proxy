//! Configuration structures for Spring Proxy.
//!
//! Mirrors ZBProxy's config package: services, router rules, outbounds,
//! and access control lists.

use std::collections::HashMap;

use common::set::StringSet;
use serde::{Deserialize, Serialize};

// Root config

/// Top-level configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Root {
    pub log: LogConfig,
    pub services: Vec<ServiceConfig>,
    pub router: RouterConfig,
    pub outbounds: Vec<OutboundConfig>,
    pub lists: HashMap<String, StringSet>,
}

// Log config

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    pub level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
        }
    }
}

// Service config

/// A listening service configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServiceConfig {
    pub name: String,
    /// Target address for legacy mode.
    pub target_address: String,
    /// Target port for legacy mode.
    pub target_port: u16,
    /// Port to listen on.
    pub listen: u16,
    /// Enable PROXY protocol on inbound connections.
    pub enable_proxy_protocol: bool,
    /// IP access control.
    pub ip_access: AccessConfig,
    /// Minecraft legacy service options.
    pub minecraft: Option<MinecraftServiceConfig>,
    /// Outbound reference for routing mode.
    pub outbound: String,
}

// Access config

/// Access control configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AccessConfig {
    pub mode: String,
    #[serde(default)]
    pub list_tags: Vec<String>,
    #[serde(default)]
    pub lower_case: bool,
}

// Minecraft service config

/// Minecraft-specific service options.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MinecraftServiceConfig {
    pub enable_hostname_rewrite: bool,
    pub rewritten_hostname: String,
    pub online_count: OnlineCountConfig,
    pub ignore_fml_suffix: bool,
    pub ignore_srv_redirect: bool,
    pub hostname_access: AccessConfig,
    pub name_access: AccessConfig,
    pub ping_mode: String,
    pub motd_favicon: String,
    pub motd_description: String,
}

/// Online player count configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OnlineCountConfig {
    pub max: i32,
    pub online: i32,
    #[serde(default)]
    pub enable_max_limit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample: Option<Vec<PlayerSampleConfig>>,
}

/// A player sample entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSampleConfig {
    pub name: String,
    pub id: String,
}

// Router config

/// Router configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RouterConfig {
    pub default_outbound: String,
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
}

/// A single routing rule.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleConfig {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<serde_json::Value>,
    #[serde(default)]
    pub rewrite: RuleRewrite,
    #[serde(default)]
    pub sniff: Vec<String>,
    pub outbound: String,
    #[serde(default)]
    pub invert: bool,
}

/// Rule rewrite configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleRewrite {
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub target_address: String,
    #[serde(default)]
    pub target_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minecraft: Option<RuleRewriteMinecraft>,
}

/// Minecraft-specific rewrite.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleRewriteMinecraft {
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub hostname: String,
    #[serde(default)]
    pub port: u16,
    pub intent: i8,
}

// Outbound config

/// Outbound configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OutboundConfig {
    pub name: String,
    /// Reference to another outbound dialer.
    pub dialer: String,
    pub target_address: String,
    pub target_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minecraft: Option<MinecraftOutboundConfig>,
    /// PROXY protocol version: 0 = unspecified, 1 = v1, 2 = v2.
    pub proxy_protocol_version: i8,
    #[serde(default)]
    pub proxy_options: ProxyOptionsConfig,
}

/// Minecraft-specific outbound configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MinecraftOutboundConfig {
    pub enable_hostname_rewrite: bool,
    pub rewritten_hostname: String,
    pub online_count: OnlineCountConfig,
    pub ignore_fml_suffix: bool,
    pub ignore_srv_redirect: bool,
    pub hostname_access: AccessConfig,
    pub name_access: AccessConfig,
    pub ping_mode: String,
    pub motd_favicon: String,
    pub motd_description: String,
}

/// Proxy options (SOCKS, etc.).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyOptionsConfig {
    pub r#type: String,
    pub network: String,
    pub address: String,
}

// Default config generation

impl Root {
    /// Generate a default configuration with a sample Hypixel proxy setup.
    pub fn generate_default() -> Self {
        Self {
            log: LogConfig::default(),
            services: vec![ServiceConfig {
                name: "Hypixel-in".into(),
                listen: 25565,
                ..ServiceConfig::default()
            }],
            router: RouterConfig {
                default_outbound: "RESET".into(),
                rules: vec![
                    RuleConfig {
                        r#type: "always".into(),
                        sniff: vec!["minecraft".into()],
                        ..RuleConfig::default()
                    },
                    RuleConfig {
                        r#type: "ServiceName".into(),
                        parameter: Some(serde_json::json!("Hypixel-in")),
                        rewrite: RuleRewrite {
                            minecraft: Some(RuleRewriteMinecraft {
                                hostname: "mc.hypixel.net".into(),
                                port: 25565,
                                ..RuleRewriteMinecraft::default()
                            }),
                            ..RuleRewrite::default()
                        },
                        outbound: "Hypixel-out".into(),
                        ..RuleConfig::default()
                    },
                ],
            },
            outbounds: vec![OutboundConfig {
                name: "Hypixel-out".into(),
                target_address: "mc.hypixel.net".into(),
                target_port: 25565,
                minecraft: Some(MinecraftOutboundConfig {
                    online_count: OnlineCountConfig {
                        max: 20,
                        online: -1,
                        ..OnlineCountConfig::default()
                    },
                    motd_favicon: "{DEFAULT_MOTD}".into(),
                    motd_description:
                        "§d{NAME}§e, provided by §a§o{INFO}§r\n§c§lProxy for §6§n{HOST}:{PORT}§r"
                            .into(),
                    ..MinecraftOutboundConfig::default()
                }),
                ..OutboundConfig::default()
            }],
            lists: HashMap::new(),
        }
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_root_serializes() {
        let root = Root::generate_default();
        let toml_str = toml::to_string_pretty(&root).unwrap();
        assert!(toml_str.contains("Hypixel-in"));
        assert!(toml_str.contains("Hypixel-out"));
    }

    #[test]
    fn roundtrip_toml() {
        let root = Root::generate_default();
        let toml_str = toml::to_string(&root).unwrap();
        let decoded: Root = toml::from_str(&toml_str).unwrap();
        assert_eq!(decoded.services.len(), 1);
        assert_eq!(decoded.outbounds.len(), 1);
    }
}
