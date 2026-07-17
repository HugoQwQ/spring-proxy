//! Kick message generation for Minecraft connections.
//!
//! Provides pre-built [`Message`](crate::message::Message) trees for
//! disconnecting players with a friendly, informative reason.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::message::*;

/// Generate a kick message for players rejected by access control.
///
/// # Arguments
/// * `outbound_name` - The name of the outbound/service that rejected the player.
/// * `player_name` - The name of the player being kicked.
pub fn generate_kick_message(outbound_name: &str, player_name: &str) -> Message {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    Message {
        color: WHITE.into(),
        extra: vec![
            Message::bold_colored("Spring", RED),
            Message::bold("Proxy"),
            Message::text(" - "),
            Message::bold_colored("Connection Rejected\n", GOLD),
            Message::text("Reason: "),
            Message::colored(
                "You don't have permission to access this service.\n",
                LIGHT_PURPLE,
            ),
            Message::colored(
                format!(
                    "Timestamp: {timestamp} | Player Name: {player_name} | Outbound: {outbound_name}\n"
                ),
                GRAY,
            ),
        ],
        ..Message::default()
    }
}

/// Generate a kick message for players rejected due to online player limit.
///
/// # Arguments
/// * `outbound_name` - The name of the outbound/service that rejected the player.
/// * `player_name` - The name of the player being kicked.
pub fn generate_player_number_limit_exceeded_message(
    outbound_name: &str,
    player_name: &str,
) -> Message {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    Message {
        color: WHITE.into(),
        extra: vec![
            Message::bold_colored("Spring", RED),
            Message::bold("Proxy"),
            Message::text(" - "),
            Message::bold_colored("Connection Rejected\n", GOLD),
            Message::text("Reason: "),
            Message::colored(
                "Service online player number limitation exceeded.\n",
                LIGHT_PURPLE,
            ),
            Message::colored(
                format!(
                    "Timestamp: {timestamp} | Player Name: {player_name} | Outbound: {outbound_name}\n"
                ),
                GRAY,
            ),
        ],
        ..Message::default()
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kick_message_contains_expected_fields() {
        let msg = generate_kick_message("test-outbound", "TestPlayer");
        let json = msg.to_json().unwrap();
        assert!(json.contains("Connection Rejected"));
        assert!(json.contains("TestPlayer"));
        assert!(json.contains("test-outbound"));
    }

    #[test]
    fn limit_exceeded_message_contains_expected_fields() {
        let msg = generate_player_number_limit_exceeded_message("test-outbound", "TestPlayer");
        let json = msg.to_json().unwrap();
        assert!(json.contains("Connection Rejected"));
        assert!(json.contains("TestPlayer"));
        assert!(json.contains("test-outbound"));
        assert!(json.contains("limitation exceeded"));
    }

    #[test]
    fn kick_message_is_valid_json() {
        let msg = generate_kick_message("outbound", "Player");
        let bytes = msg.to_json_bytes().unwrap();
        let _: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    }
}
