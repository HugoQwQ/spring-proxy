//! Enhanced Minecraft client handshake sniffing.
//!
//! Provides [`sniff_client_handshake`], which reads a Minecraft handshake
//! packet (and, for login connections, the Login Start packet) from an
//! async stream and returns a populated [`MinecraftMetadata`].
//!
//! All bytes consumed from the stream are appended to the provided buffer
//! so they can be replayed to the backend server.

use common::{Buf, BytesMut, Error, VarInt};
use smol::io::{AsyncRead, AsyncReadExt};

use crate::metadata::MinecraftMetadata;
use crate::packet::{self, MAX_HANDSHAKE_PACKET_SIZE};

/// Error returned when a packet does not look like a valid Minecraft handshake.
pub const ERR_BAD_PACKET: &str = "bad Minecraft handshake packet";

/// Sniff a Minecraft client handshake from an async reader.
///
/// Reads the handshake packet (and, for login connections, the Login Start
/// packet) and populates a [`MinecraftMetadata`]. All consumed bytes are
/// appended to `replay_buf` so they can be sent to the backend server later.
///
/// # Protocol version-dependent behaviour
/// - **≥ 764 (1.20.2):** UUID is expected to always be present (16 bytes).
/// - **≥ 761 (1.19.3):** UUID has a boolean prefix.
/// - **≥ 759 (1.19):** UUID has a boolean prefix after optional signature data.
/// - **< 759:** UUID is not sent by the client.
///
/// # Errors
/// Returns [`Error::Protocol`] with [`ERR_BAD_PACKET`] if the stream does not
/// contain a valid Minecraft handshake.
pub async fn sniff_client_handshake<R: AsyncRead + Unpin>(
    reader: &mut R,
    replay_buf: &mut BytesMut,
) -> Result<MinecraftMetadata, Error> {
    let mut metadata = MinecraftMetadata::default();

    // Handshake packet
    let body = read_raw_packet(reader, replay_buf, MAX_HANDSHAKE_PACKET_SIZE).await?;

    // Parse packet body from a local buffer so we don't advance replay_buf
    let mut parse_buf = body.clone();

    // Packet ID must be 0x00 (Server-bound Handshake)
    let packet_id = packet::read_byte(&mut parse_buf)?;
    if packet_id != 0x00 {
        return Err(Error::Protocol(ERR_BAD_PACKET.into()));
    }

    // Protocol version
    let (protocol_version, _) = VarInt::decode(&parse_buf)?;
    if protocol_version.0 <= 0 {
        return Err(Error::Protocol(ERR_BAD_PACKET.into()));
    }
    metadata.protocol_version = protocol_version.0;
    let pv_size = VarInt(protocol_version.0).encoded_size();
    parse_buf.advance(pv_size);

    // Origin destination
    metadata.origin_destination = packet::read_string(&mut parse_buf)?;
    if metadata.origin_destination.is_empty() {
        return Err(Error::Protocol(ERR_BAD_PACKET.into()));
    }

    // Origin port
    metadata.origin_port = packet::read_u16_be(&mut parse_buf)?;
    if metadata.origin_port == 0 {
        return Err(Error::Protocol(ERR_BAD_PACKET.into()));
    }

    // Next state / intent
    let next_state = packet::read_byte(&mut parse_buf)?;
    match next_state {
        1..=3 => metadata.next_state = next_state as i8,
        _ => return Err(Error::Protocol(ERR_BAD_PACKET.into())),
    }

    metadata.sniff_position = replay_buf.len();

    // Post-handshake packets
    match metadata.next_state {
        // Status request
        1 => {
            // Status request packet is 2 bytes: [0x01, 0x00]
            let mut status_req = [0u8; 2];
            reader.read_exact(&mut status_req).await?;
            replay_buf.extend_from_slice(&status_req);
            // Validate: packet length 1, packet ID 0x00
            // (We don't strictly validate here; ZBProxy just peeks.)
        }

        // Login start
        2 => {
            // Read login start packet
            let login_body = read_raw_packet(reader, replay_buf, MAX_HANDSHAKE_PACKET_SIZE).await?;
            let mut login_parse = login_body;

            // Packet ID must be 0x00 (Server-bound Login Start)
            let login_packet_id = packet::read_byte(&mut login_parse)?;
            if login_packet_id != 0x00 {
                return Err(Error::Protocol(ERR_BAD_PACKET.into()));
            }

            // Player name (max 16 chars)
            metadata.player_name = packet::read_limited_string(&mut login_parse, 16)?;

            // UUID parsing (version-dependent)
            if metadata.protocol_version >= 764 {
                // 1.20.2+: UUID always present, 16 bytes
                if login_parse.len() >= 16 {
                    metadata.uuid.copy_from_slice(&login_parse[..16]);
                }
            } else if metadata.protocol_version >= 761 {
                // 1.19.3+: UUID has boolean prefix
                if !login_parse.is_empty() {
                    let has_uuid = packet::read_byte(&mut login_parse)?;
                    if has_uuid == 0x01 && login_parse.len() >= 16 {
                        metadata.uuid.copy_from_slice(&login_parse[..16]);
                    }
                }
            } else if metadata.protocol_version >= 759 {
                // 1.19+: Signature data + UUID with boolean prefix
                if !login_parse.is_empty() {
                    let has_sig_data = packet::read_byte(&mut login_parse)?;
                    if has_sig_data == 0x01 {
                        // Skip timestamp (8 bytes)
                        if login_parse.len() < 8 {
                            return Ok(metadata);
                        }
                        login_parse.advance(8);
                        // Skip public key
                        let (pk_len, _) = VarInt::decode(&login_parse)?;
                        let pk_size = VarInt(pk_len.0).encoded_size();
                        let pk_total = pk_size + pk_len.0 as usize;
                        if login_parse.len() < pk_total {
                            return Ok(metadata);
                        }
                        login_parse.advance(pk_total);
                        // Skip signature
                        let (sig_len, _) = VarInt::decode(&login_parse)?;
                        let sig_size = VarInt(sig_len.0).encoded_size();
                        let sig_total = sig_size + sig_len.0 as usize;
                        if login_parse.len() < sig_total {
                            return Ok(metadata);
                        }
                        login_parse.advance(sig_total);
                    }
                    // UUID boolean prefix
                    if !login_parse.is_empty() {
                        let has_uuid = packet::read_byte(&mut login_parse)?;
                        if has_uuid == 0x01 && login_parse.len() >= 16 {
                            metadata.uuid.copy_from_slice(&login_parse[..16]);
                        }
                    }
                }
            }
        }

        // Transfer (1.20.5+)
        3 => {
            // TODO: Transfer packet handling
        }

        _ => {}
    }

    Ok(metadata)
}

/// Read a raw length-prefixed packet from an async reader.
///
/// The raw bytes (length prefix + body) are appended to `replay_buf`.
/// The packet body (without length prefix) is returned for parsing.
async fn read_raw_packet<R: AsyncRead + Unpin>(
    reader: &mut R,
    replay_buf: &mut BytesMut,
    max_len: i32,
) -> Result<BytesMut, Error> {
    // Read length VarInt byte by byte so we can store the raw prefix
    let mut len_buf = [0u8; 5];
    let mut len_bytes_read = 0;
    let mut packet_len = 0i32;

    for i in 0..5 {
        let mut b = [0u8; 1];
        reader.read_exact(&mut b).await?;
        len_buf[i] = b[0];
        len_bytes_read += 1;
        packet_len |= ((b[0] & 0x7F) as i32) << (i * 7);
        if (b[0] & 0x80) == 0 {
            break;
        }
    }

    if packet_len < 0 {
        return Err(Error::Protocol(format!(
            "incorrect packet length: {packet_len}"
        )));
    }
    if packet_len > max_len {
        return Err(Error::Protocol(format!(
            "packet max length exceeded: length={packet_len}, max={max_len}"
        )));
    }

    // Read body
    let mut body = vec![0u8; packet_len as usize];
    reader.read_exact(&mut body).await?;

    // Append raw packet to replay buffer
    replay_buf.extend_from_slice(&len_buf[..len_bytes_read]);
    replay_buf.extend_from_slice(&body);

    Ok(BytesMut::from(&body[..]))
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet;

    /// Build a raw handshake packet (including length prefix).
    fn build_handshake_packet(
        protocol_version: i32,
        address: &str,
        port: u16,
        next_state: u8,
    ) -> Vec<u8> {
        let mut body = BytesMut::new();
        packet::write_byte(&mut body, 0x00); // packet ID
        let mut tmp = [0u8; 5];
        let n = VarInt(protocol_version).encode(&mut tmp).unwrap();
        body.extend_from_slice(&tmp[..n]);
        packet::write_string(&mut body, address);
        packet::write_u16_be(&mut body, port);
        packet::write_byte(&mut body, next_state);

        let mut packet = BytesMut::new();
        let n = VarInt(body.len() as i32).encode(&mut tmp).unwrap();
        packet.extend_from_slice(&tmp[..n]);
        packet.extend_from_slice(&body);
        packet.to_vec()
    }

    /// Build a raw login start packet (including length prefix).
    fn build_login_start_packet(player_name: &str, uuid: Option<[u8; 16]>) -> Vec<u8> {
        let mut body = BytesMut::new();
        packet::write_byte(&mut body, 0x00); // packet ID
        packet::write_string(&mut body, player_name);
        if let Some(u) = uuid {
            body.extend_from_slice(&u);
        }

        let mut packet = BytesMut::new();
        let mut tmp = [0u8; 5];
        let n = VarInt(body.len() as i32).encode(&mut tmp).unwrap();
        packet.extend_from_slice(&tmp[..n]);
        packet.extend_from_slice(&body);
        packet.to_vec()
    }

    #[test]
    fn sniff_status_handshake() {
        smol::block_on(async {
            let hs = build_handshake_packet(763, "hypixel.net", 25565, 1);
            let status_req = [0x01, 0x00];
            let mut data = hs.clone();
            data.extend_from_slice(&status_req);

            let mut cursor = smol::io::Cursor::new(data);
            let mut replay = BytesMut::new();
            let meta = sniff_client_handshake(&mut cursor, &mut replay)
                .await
                .unwrap();

            assert_eq!(meta.protocol_version, 763);
            assert_eq!(meta.origin_destination, "hypixel.net");
            assert_eq!(meta.origin_port, 25565);
            assert_eq!(meta.next_state, 1);
            assert!(meta.player_name.is_empty());
            // Replay buffer should contain everything
            assert_eq!(replay.len(), hs.len() + 2);
        });
    }

    #[test]
    fn sniff_login_handshake_modern_uuid() {
        smol::block_on(async {
            let hs = build_handshake_packet(764, "mc.example.com", 25565, 2);
            let uuid: [u8; 16] = [
                0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
                0x0F, 0x10,
            ];
            let login = build_login_start_packet("TestPlayer", Some(uuid));
            let mut data = hs.clone();
            data.extend_from_slice(&login);

            let mut cursor = smol::io::Cursor::new(data);
            let mut replay = BytesMut::new();
            let meta = sniff_client_handshake(&mut cursor, &mut replay)
                .await
                .unwrap();

            assert_eq!(meta.protocol_version, 764);
            assert_eq!(meta.origin_destination, "mc.example.com");
            assert_eq!(meta.next_state, 2);
            assert_eq!(meta.player_name, "TestPlayer");
            assert_eq!(meta.uuid, uuid);
        });
    }

    #[test]
    fn sniff_login_handshake_old_no_uuid() {
        smol::block_on(async {
            let hs = build_handshake_packet(47, "old.server.com", 25565, 2);
            let login = build_login_start_packet("OldPlayer", None);
            let mut data = hs.clone();
            data.extend_from_slice(&login);

            let mut cursor = smol::io::Cursor::new(data);
            let mut replay = BytesMut::new();
            let meta = sniff_client_handshake(&mut cursor, &mut replay)
                .await
                .unwrap();

            assert_eq!(meta.protocol_version, 47);
            assert_eq!(meta.player_name, "OldPlayer");
            assert_eq!(meta.uuid, [0u8; 16]);
        });
    }

    #[test]
    fn sniff_bad_packet_id() {
        smol::block_on(async {
            let mut body = BytesMut::new();
            packet::write_byte(&mut body, 0x01); // wrong packet ID
            let mut packet = BytesMut::new();
            let mut tmp = [0u8; 5];
            let n = VarInt(body.len() as i32).encode(&mut tmp).unwrap();
            packet.extend_from_slice(&tmp[..n]);
            packet.extend_from_slice(&body);

            let mut cursor = smol::io::Cursor::new(packet.to_vec());
            let mut replay = BytesMut::new();
            assert!(
                sniff_client_handshake(&mut cursor, &mut replay)
                    .await
                    .is_err()
            );
        });
    }

    #[test]
    fn sniff_fml_detection() {
        smol::block_on(async {
            let hs = build_handshake_packet(763, "hypixel.net\x00FML\x01", 25565, 2);
            let login = build_login_start_packet("Player", None);
            let mut data = hs.clone();
            data.extend_from_slice(&login);

            let mut cursor = smol::io::Cursor::new(data);
            let mut replay = BytesMut::new();
            let meta = sniff_client_handshake(&mut cursor, &mut replay)
                .await
                .unwrap();

            assert!(meta.is_fml());
            assert_eq!(meta.clean_origin_destination(), "hypixel.net");
            assert_eq!(meta.fml_markup(), "FML\x01");
        });
    }
}
