//! Pure Minecraft packet builders.

use common::{BytesMut, Error, VarInt};
use protocol::packet;

/// Build a server-bound handshake packet (including length prefix).
pub fn build_handshake_packet(
    protocol_version: i32,
    hostname: &str,
    port: u16,
    intent: u8,
) -> Result<BytesMut, Error> {
    let mut body = BytesMut::new();
    packet::write_byte(&mut body, 0x00); // packet ID
    let mut tmp = [0u8; 5];
    let n = VarInt(protocol_version).encode(&mut tmp)?;
    body.extend_from_slice(&tmp[..n]);
    packet::write_string(&mut body, hostname);
    packet::write_u16_be(&mut body, port);
    packet::write_byte(&mut body, intent);

    let mut packet_buf = BytesMut::new();
    let n = VarInt(body.len() as i32).encode(&mut tmp)?;
    packet_buf.extend_from_slice(&tmp[..n]);
    packet_buf.extend_from_slice(&body);

    Ok(packet_buf)
}

/// Build a client-bound disconnect/kick packet (login phase).
pub fn build_kick_packet(message_json: &str) -> Result<BytesMut, Error> {
    let mut body = BytesMut::new();
    packet::write_byte(&mut body, 0x00); // Disconnect (login)
    packet::write_string(&mut body, message_json);

    let mut packet_buf = BytesMut::new();
    let mut tmp = [0u8; 5];
    let n = VarInt(body.len() as i32).encode(&mut tmp)?;
    packet_buf.extend_from_slice(&tmp[..n]);
    packet_buf.extend_from_slice(&body);

    Ok(packet_buf)
}

/// Build a client-bound status response packet.
pub fn build_status_response_packet(motd_json: &str) -> Result<BytesMut, Error> {
    let mut body = BytesMut::new();
    packet::write_byte(&mut body, 0x00); // Status Response
    packet::write_string(&mut body, motd_json);

    let mut packet_buf = BytesMut::new();
    let mut tmp = [0u8; 5];
    let n = VarInt(body.len() as i32).encode(&mut tmp)?;
    packet_buf.extend_from_slice(&tmp[..n]);
    packet_buf.extend_from_slice(&body);

    Ok(packet_buf)
}

/// Build a client-bound ping response (pong) packet.
pub fn build_ping_pong_packet(timestamp: u64) -> Result<BytesMut, Error> {
    let mut body = BytesMut::new();
    packet::write_byte(&mut body, 0x01); // Ping Response
    packet::write_u64_be(&mut body, timestamp);

    let mut packet_buf = BytesMut::new();
    let mut tmp = [0u8; 5];
    let n = VarInt(body.len() as i32).encode(&mut tmp)?;
    packet_buf.extend_from_slice(&tmp[..n]);
    packet_buf.extend_from_slice(&body);

    Ok(packet_buf)
}

/// Build a server-bound login start packet (including length prefix).
pub fn build_login_start_packet(
    player_name: &str,
    uuid: &[u8; 16],
    protocol_version: i32,
) -> Result<BytesMut, Error> {
    let mut body = BytesMut::new();
    packet::write_byte(&mut body, 0x00); // Login Start
    packet::write_string(&mut body, player_name);
    if protocol_version >= 764 && *uuid != [0u8; 16] {
        body.extend_from_slice(uuid);
    }

    let mut packet_buf = BytesMut::new();
    let mut tmp = [0u8; 5];
    let n = VarInt(body.len() as i32).encode(&mut tmp)?;
    packet_buf.extend_from_slice(&tmp[..n]);
    packet_buf.extend_from_slice(&body);

    Ok(packet_buf)
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::packet;

    #[test]
    fn handshake_packet_structure() {
        let pkt = build_handshake_packet(763, "hypixel.net", 25565, 2).unwrap();
        // First bytes should be a VarInt length prefix
        let (len, consumed) = VarInt::decode(&pkt).unwrap();
        assert_eq!(len.0 as usize, pkt.len() - consumed);

        // After length: packet ID 0x00
        let (id, _id_consumed) = VarInt::decode(&pkt[consumed..]).unwrap();
        assert_eq!(id.0, 0x00);

        // Then protocol version
        let (ver, _) = VarInt::decode(&pkt[consumed + _id_consumed..]).unwrap();
        assert_eq!(ver.0, 763);
    }

    #[test]
    fn kick_packet_structure() {
        let pkt = build_kick_packet(r#"{"text":"Bad"}"#).unwrap();
        let (len, consumed) = VarInt::decode(&pkt).unwrap();
        assert_eq!(len.0 as usize, pkt.len() - consumed);

        let (id, _id_consumed) = VarInt::decode(&pkt[consumed..]).unwrap();
        assert_eq!(id.0, 0x00); // Disconnect
    }

    #[test]
    fn status_response_structure() {
        let pkt = build_status_response_packet(r#"{"description":"Hi"}"#).unwrap();
        let (len, consumed) = VarInt::decode(&pkt).unwrap();
        assert_eq!(len.0 as usize, pkt.len() - consumed);

        let (id, _id_consumed) = VarInt::decode(&pkt[consumed..]).unwrap();
        assert_eq!(id.0, 0x00); // Status Response
    }

    #[test]
    fn ping_pong_structure() {
        let pkt = build_ping_pong_packet(12345).unwrap();
        let (len, consumed) = VarInt::decode(&pkt).unwrap();
        assert_eq!(len.0 as usize, pkt.len() - consumed);

        let (id, _id_consumed) = VarInt::decode(&pkt[consumed..]).unwrap();
        assert_eq!(id.0, 0x01); // Ping Response
    }

    #[test]
    fn login_start_with_uuid() {
        let uuid = [0x01u8; 16];
        let pkt = build_login_start_packet("TestPlayer", &uuid, 764).unwrap();
        let (_len, consumed) = VarInt::decode(&pkt).unwrap();
        assert_eq!(_len.0 as usize, pkt.len() - consumed);

        let (id, _id_consumed) = VarInt::decode(&pkt[consumed..]).unwrap();
        assert_eq!(id.0, 0x00); // Login Start

        // Verify player name is in there
        let body = &pkt[consumed + _id_consumed..];
        let mut buf = common::BytesMut::from(body);
        let name = packet::read_string(&mut buf).unwrap();
        assert_eq!(name, "TestPlayer");

        // Modern protocol includes UUID
        assert_eq!(&buf[..16], &uuid[..]);
    }

    #[test]
    fn login_start_without_uuid_old_proto() {
        let uuid = [0x01u8; 16];
        let pkt = build_login_start_packet("OldPlayer", &uuid, 47).unwrap();
        let (_len, consumed) = VarInt::decode(&pkt).unwrap();

        let (id, _id_consumed) = VarInt::decode(&pkt[consumed..]).unwrap();
        assert_eq!(id.0, 0x00);

        let body = &pkt[consumed + _id_consumed..];
        let mut buf = common::BytesMut::from(body);
        let name = packet::read_string(&mut buf).unwrap();
        assert_eq!(name, "OldPlayer");

        // Old protocol: no UUID appended
        assert!(buf.is_empty());
    }
}
