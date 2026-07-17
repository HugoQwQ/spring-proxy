//! Minecraft connection metadata.
//!
//! Holds all information sniffed from a Minecraft client's handshake
//! and login start packets, including protocol version, player name,
//! UUID, origin destination, and Forge Mod Loader (FML) state.

use std::sync::OnceLock;

/// Metadata extracted from a Minecraft client's connection handshake.
///
/// Populated by the sniffing process in [`crate::sniff::sniff_client_handshake`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinecraftMetadata {
    /// Minecraft protocol version (e.g., 763 for 1.20.1).
    pub protocol_version: i32,
    /// Player name extracted from the Login Start packet.
    pub player_name: String,
    /// The hostname the client originally connected to (may contain FML markup).
    pub origin_destination: String,
    /// The hostname to send to the backend server (after rewrite).
    pub rewritten_destination: String,
    /// The port the client originally connected to.
    pub origin_port: u16,
    /// The port to send to the backend server (after rewrite).
    pub rewritten_port: u16,
    /// Player UUID (16 bytes), if present in the Login Start packet.
    pub uuid: [u8; 16],
    /// Next state / intent: 1 = status, 2 = login, 3 = transfer.
    pub next_state: i8,
    /// Byte position in the stream after sniffing is complete.
    pub sniff_position: usize,
    /// Cached FML markup (the part after `\x00` in origin_destination).
    fml_markup: OnceLock<String>,
    /// Cached clean origin destination (the part before `\x00`).
    clean_origin: OnceLock<String>,
}

impl Default for MinecraftMetadata {
    fn default() -> Self {
        Self {
            protocol_version: 0,
            player_name: String::new(),
            origin_destination: String::new(),
            rewritten_destination: String::new(),
            origin_port: 0,
            rewritten_port: 0,
            uuid: [0u8; 16],
            next_state: -1,
            sniff_position: 0,
            fml_markup: OnceLock::new(),
            clean_origin: OnceLock::new(),
        }
    }
}

impl MinecraftMetadata {
    /// Returns `true` if the metadata represents a valid Minecraft connection.
    ///
    /// A valid connection has a positive next state (status, login, or transfer).
    pub fn valid(&self) -> bool {
        self.next_state > 0
    }

    /// Returns `true` if the origin destination contains a Forge Mod Loader
    /// marker (`\x00` null byte separating the hostname from FML markup).
    pub fn is_fml(&self) -> bool {
        self.origin_destination.contains('\x00')
    }

    /// Returns the origin destination with any FML markup stripped.
    ///
    /// For example, `hypixel.net\x00FML\x01` becomes `hypixel.net`.
    pub fn clean_origin_destination(&self) -> &str {
        self.clean_origin
            .get_or_init(|| {
                self.origin_destination
                    .split('\x00')
                    .next()
                    .unwrap_or(&self.origin_destination)
                    .to_string()
            })
            .as_str()
    }

    /// Returns the FML markup portion of the origin destination.
    ///
    /// Returns an empty string if there is no FML marker.
    pub fn fml_markup(&self) -> &str {
        self.fml_markup
            .get_or_init(|| {
                self.origin_destination
                    .split_once('\x00')
                    .map(|(_, markup)| markup.to_string())
                    .unwrap_or_default()
            })
            .as_str()
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_valid() {
        let mut m = MinecraftMetadata::default();
        assert!(!m.valid());
        m.next_state = 1;
        assert!(m.valid());
        m.next_state = 2;
        assert!(m.valid());
    }

    #[test]
    fn metadata_fml_detection() {
        let mut m = MinecraftMetadata::default();
        m.origin_destination = "hypixel.net".into();
        assert!(!m.is_fml());

        m.origin_destination = "hypixel.net\x00FML\x01".into();
        assert!(m.is_fml());
        assert_eq!(m.clean_origin_destination(), "hypixel.net");
        assert_eq!(m.fml_markup(), "FML\x01");
    }

    #[test]
    fn metadata_no_fml() {
        let mut m = MinecraftMetadata::default();
        m.origin_destination = "mc.example.com".into();
        assert!(!m.is_fml());
        assert_eq!(m.clean_origin_destination(), "mc.example.com");
        assert_eq!(m.fml_markup(), "");
    }

    #[test]
    fn metadata_clean_origin_caching() {
        let mut m = MinecraftMetadata::default();
        m.origin_destination = "host\x00markup".into();
        assert_eq!(m.clean_origin_destination(), "host");
        // Second call should use cached value
        assert_eq!(m.clean_origin_destination(), "host");
    }
}
