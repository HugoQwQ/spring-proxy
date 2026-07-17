//! MOTD (Message of the Day) generation for Minecraft server list ping.
//!
//! Provides [`MotdObject`] for JSON serialization and [`generate_motd`]
//! to build a server status response matching ZBProxy's format.

use serde::Serialize;

/// A player sample entry in the MOTD response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerSample {
    pub name: String,
    pub id: String,
}

/// The JSON structure of a Minecraft server status (MOTD) response.
///
/// Serializes to the format expected by Minecraft clients in the
/// status handshake phase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MotdObject {
    pub version: MotdVersion,
    pub players: MotdPlayers,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<MotdDescription>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon: Option<String>,
}

/// Version info in the MOTD response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MotdVersion {
    pub name: String,
    pub protocol: i32,
}

/// Player count info in the MOTD response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MotdPlayers {
    pub max: i32,
    pub online: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample: Option<Vec<PlayerSample>>,
}

/// Description text in the MOTD response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MotdDescription {
    pub text: String,
}

/// Generate a MOTD JSON response.
///
/// # Arguments
/// * `protocol_version` - The Minecraft protocol version to report.
/// * `version_name` - The server version name string (e.g., "Spring Proxy 0.1.0").
/// * `description` - The MOTD description text.
/// * `max_players` - Maximum player slots to report.
/// * `online_players` - Current online player count to report.
/// * `favicon` - Optional base64-encoded favicon data URI.
/// * `sample` - Optional player sample list.
///
/// # Returns
/// A JSON byte vector suitable for sending in a Status Response packet.
pub fn generate_motd(
    protocol_version: i32,
    version_name: &str,
    description: &str,
    max_players: i32,
    online_players: i32,
    favicon: Option<&str>,
    sample: Option<Vec<PlayerSample>>,
) -> Vec<u8> {
    let motd = MotdObject {
        version: MotdVersion {
            name: version_name.into(),
            protocol: protocol_version,
        },
        players: MotdPlayers {
            max: max_players,
            online: online_players,
            sample,
        },
        description: if description.is_empty() {
            None
        } else {
            Some(MotdDescription {
                text: description.into(),
            })
        },
        favicon: favicon.map(|s| s.into()),
    };

    serde_json::to_vec(&motd).unwrap_or_default()
}

/// Default favicon base64 PNG, used when no custom favicon is set.
pub const DEFAULT_MOTD_FAVICON: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAYAAACqaXHeAAACLUlEQVR4AeyYi26jMBBFc/r//7y7p9ZsW0QJGD+BSFeWydjMPTMmER9/bv75eN388wC4eQO8ng54OuDmBJ4j0KIBgBe8V4tclveo0gHw0+y//1qvPVom12JeFAAk40uzLYzk3qMYAOB/lXOT6bGuCABI5nsYOHvP0wBgXvPCOw3ATWbWKQAwX/WXxcoGAPObF0Y2ABdfQVkA4BrVt4BZAPyjA7h+emUBmN71NwPZAK7SBdkAhBgQYN7jcApAQJgZxGkAQlBCUDBXNxQDIAQVEGAOEMUBBIRZQFQBIAQlBAXjdkNVAEJQAQHGA9EEQEAYEUQzAEJQQlDA55tir/VUcwBhVggK6oKI+/02dgMQCQlBQQIBxFdNxu4AwqUQQkCz4zEMgADh2BLEkACEoFqAGBqAENR3EM5LagoAYVgQQEyLjFMBKOJ4sclUAKD8y9gpAACfP4segUUBT0+HBwCp6jXMS294ACZZU0MDgFT9WwKA+uYFu9kBgDFNBVR74K0Z2QTgAsChiSBVvdYDb83EJoCWiUAyv5bkkWtHYzcBHN0sNx76mDfftwDsAqh3DKCf+V0ADKoBAWj6sNPHmt52QCwqBQG+jLtn7N9r3A3ABE0YkgHnRwRpnXuoI2trxh4CYCImryAZ8tqWIMW5Rm3F9vjuMIBIUjMKkkFYH41RsW60MRtAGNHcliJu1PE0gFGN7c3rAbCX1FXjng64amX3+no6YC+pq8ZN3wFnC/MXAAD//4HosP8AAAAGSURBVAMAKaoNn2qpo/QAAAAASUVORK5CYII=";

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_motd_basic() {
        let motd = generate_motd(
            763,
            "Spring Proxy 0.1.0",
            "A Minecraft proxy",
            100,
            42,
            None,
            None,
        );
        let json: serde_json::Value = serde_json::from_slice(&motd).unwrap();
        assert_eq!(json["version"]["protocol"], 763);
        assert_eq!(json["version"]["name"], "Spring Proxy 0.1.0");
        assert_eq!(json["players"]["max"], 100);
        assert_eq!(json["players"]["online"], 42);
        assert_eq!(json["description"]["text"], "A Minecraft proxy");
        assert!(json.get("favicon").is_none());
    }

    #[test]
    fn generate_motd_with_favicon_and_sample() {
        let sample = vec![PlayerSample {
            name: "Player1".into(),
            id: "00000000-0000-0000-0000-000000000001".into(),
        }];
        let motd = generate_motd(
            47,
            "Test",
            "Hello",
            10,
            1,
            Some("data:image/png;base64,abc"),
            Some(sample),
        );
        let json: serde_json::Value = serde_json::from_slice(&motd).unwrap();
        assert_eq!(json["favicon"], "data:image/png;base64,abc");
        assert_eq!(json["players"]["sample"][0]["name"], "Player1");
    }

    #[test]
    fn generate_motd_empty_description() {
        let motd = generate_motd(763, "Test", "", 10, 0, None, None);
        let json: serde_json::Value = serde_json::from_slice(&motd).unwrap();
        assert!(json.get("description").is_none());
    }
}
