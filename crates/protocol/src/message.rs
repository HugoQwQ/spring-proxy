//! Minecraft chat message format.
//!
//! Provides the [`Message`] struct for serializing Minecraft chat components
//! to JSON, used in MOTD responses, kick messages, and other player-facing text.
//!
//! Based on the Minecraft chat component format:
//! <https://wiki.vg/Chat>

// Colors

/// Minecraft chat color: `black`.
pub const BLACK: &str = "black";
/// Minecraft chat color: `dark_blue`.
pub const DARK_BLUE: &str = "dark_blue";
/// Minecraft chat color: `dark_green`.
pub const DARK_GREEN: &str = "dark_green";
/// Minecraft chat color: `dark_aqua`.
pub const DARK_AQUA: &str = "dark_aqua";
/// Minecraft chat color: `dark_red`.
pub const DARK_RED: &str = "dark_red";
/// Minecraft chat color: `dark_purple`.
pub const DARK_PURPLE: &str = "dark_purple";
/// Minecraft chat color: `gold`.
pub const GOLD: &str = "gold";
/// Minecraft chat color: `gray`.
pub const GRAY: &str = "gray";
/// Minecraft chat color: `dark_gray`.
pub const DARK_GRAY: &str = "dark_gray";
/// Minecraft chat color: `blue`.
pub const BLUE: &str = "blue";
/// Minecraft chat color: `green`.
pub const GREEN: &str = "green";
/// Minecraft chat color: `aqua`.
pub const AQUA: &str = "aqua";
/// Minecraft chat color: `red`.
pub const RED: &str = "red";
/// Minecraft chat color: `light_purple`.
pub const LIGHT_PURPLE: &str = "light_purple";
/// Minecraft chat color: `yellow`.
pub const YELLOW: &str = "yellow";
/// Minecraft chat color: `white`.
pub const WHITE: &str = "white";

// Message

/// A Minecraft chat message component.
///
/// Serializes to the JSON chat component format used by the Minecraft protocol.
/// Supports text, colors, formatting, and nested `extra` components.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Message {
    /// The plain text content of this component.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub text: String,

    /// Whether the text is bold.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub bold: bool,
    /// Whether the text is italic.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub italic: bool,
    /// Whether the text is underlined.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub underlined: bool,
    /// Whether the text has strikethrough.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub strikethrough: bool,
    /// Whether the text is obfuscated.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub obfuscated: bool,

    /// Font override (e.g. `minecraft:uniform`, `minecraft:alt`, `minecraft:default`).
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub font: String,
    /// Text color (Minecraft color name or hex `#RRGGBB`).
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub color: String,

    /// Text to insert when shift-clicked.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub insertion: String,

    /// Translation key (for i18n messages).
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub translate: String,
    /// Arguments for the translation key.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub with: Vec<Message>,
    /// Additional message components appended after this one.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub extra: Vec<Message>,
}

impl Message {
    /// Create a simple text message.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// Create a text message with a color.
    pub fn colored(text: impl Into<String>, color: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: color.into(),
            ..Self::default()
        }
    }

    /// Create a bold text message.
    pub fn bold(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: true,
            ..Self::default()
        }
    }

    /// Create a bold text message with a color.
    pub fn bold_colored(text: impl Into<String>, color: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: true,
            color: color.into(),
            ..Self::default()
        }
    }

    /// Serialize this message to a JSON string.
    pub fn to_json(&self) -> Result<String, common::Error> {
        serde_json::to_string(self)
            .map_err(|e| common::Error::Protocol(format!("JSON serialize error: {e}")))
    }

    /// Serialize this message to a JSON byte vector.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, common::Error> {
        serde_json::to_vec(self)
            .map_err(|e| common::Error::Protocol(format!("JSON serialize error: {e}")))
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_simple_serialize() {
        let msg = Message::text("Hello, world!");
        let json = msg.to_json().unwrap();
        assert_eq!(json, r#"{"text":"Hello, world!"}"#);
    }

    #[test]
    fn message_colored_serialize() {
        let msg = Message::colored("Hello", RED);
        let json = msg.to_json().unwrap();
        assert_eq!(json, r#"{"text":"Hello","color":"red"}"#);
    }

    #[test]
    fn message_with_extra() {
        let msg = Message {
            text: "Prefix ".into(),
            color: WHITE.into(),
            extra: vec![Message::bold_colored("Spring", RED), Message::bold("Proxy")],
            ..Message::default()
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("Prefix "));
        assert!(json.contains("Spring"));
        assert!(json.contains("Proxy"));
        assert!(json.contains("red"));
        assert!(json.contains("white"));
    }

    #[test]
    fn message_roundtrip() {
        let msg = Message {
            text: "Test".into(),
            bold: true,
            italic: true,
            color: GOLD.into(),
            extra: vec![Message::colored("Extra", AQUA)],
            ..Message::default()
        };
        let json = msg.to_json().unwrap();
        let decoded: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.text, "Test");
        assert!(decoded.bold);
        assert!(decoded.italic);
        assert_eq!(decoded.color, GOLD);
        assert_eq!(decoded.extra.len(), 1);
        assert_eq!(decoded.extra[0].text, "Extra");
        assert_eq!(decoded.extra[0].color, AQUA);
    }
}
