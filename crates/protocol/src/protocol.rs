//! Minecraft protocol types, packet I/O, handshake sniffing, and server responses.
//!
//! This crate provides:
//! - [`Handshake`] and [`NextState`] — the basic handshake packet format.
//! - [`MinecraftStream`] — a stream wrapper that sniffs the handshake.
//! - [`packet`] — low-level Minecraft packet I/O (VarInt, length-prefixed packets, strings).
//! - [`message`] — Minecraft chat component JSON format.
//! - [`metadata`] — [`MinecraftMetadata`] with FML detection.
//! - [`sniff`] — enhanced async handshake sniffing with player name and UUID extraction.
//! - [`motd`] — MOTD (server status) JSON generation.
//! - [`kick`] — kick message generation for access control.
//!
//! # Depth
//! Small public interface — a handful of types and functions.
//! Behind it: full Minecraft protocol parsing, version-dependent UUID decoding,
//! Forge Mod Loader detection, JSON chat serialization.

#![allow(clippy::style)]

use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

use common::{Buf, BytesMut, Error, VarInt};
use smol::io::{AsyncRead, AsyncWrite};

pub mod kick;
pub mod message;
pub mod metadata;
pub mod motd;
pub mod packet;
pub mod ping;
pub mod sniff;

// Re-export commonly used types
pub use metadata::MinecraftMetadata;
pub use sniff::sniff_client_handshake;

// Handshake

/// A decoded Minecraft handshake packet (0x00).
///
/// This is the first packet a Minecraft client sends:
/// - Protocol version (e.g., 47 for 1.8.x, 763 for 1.20.x)
/// - Server address the client typed (e.g., `hypixel.net`)
/// - Server port (usually 25565)
/// - Next state: 1 for `status` (ping), 2 for `login`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handshake {
    /// Minecraft protocol version (e.g., 763 for 1.20.1).
    pub protocol_version: i32,
    /// The hostname the client connected to.
    pub server_address: String,
    /// The port the client connected to.
    pub server_port: u16,
    /// Next state: 0x01 = status (server list ping), 0x02 = login.
    pub next_state: NextState,
}

/// The next state requested by the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextState {
    /// Server list ping (status request).
    Status = 1,
    /// Login (join server).
    Login = 2,
}

impl fmt::Display for NextState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Status => f.write_str("status"),
            Self::Login => f.write_str("login"),
        }
    }
}

impl Handshake {
    /// Try to parse a Handshake from raw packet bytes.
    ///
    /// The bytes should be a complete Minecraft packet starting after the
    /// length prefix — i.e., the packet ID (0x00) followed by VarInt fields.
    pub fn from_bytes(data: &[u8]) -> Result<Option<Self>, Error> {
        if data.is_empty() {
            return Ok(None);
        }

        // Packet ID is the first VarInt
        let (packet_id, mut offset) = VarInt::decode(data)?;
        // Handshake packet has ID 0x00
        if packet_id.0 != 0x00 {
            return Ok(None);
        }

        // Protocol version
        let (protocol_version, consumed) = VarInt::decode(&data[offset..])?;
        offset += consumed;

        // Server address (length-prefixed string)
        let (addr_len, consumed) = VarInt::decode(&data[offset..])?;
        offset += consumed;
        if offset + addr_len.0 as usize > data.len() {
            return Err(Error::InvalidHandshake(
                "server address length exceeds packet".into(),
            ));
        }
        let server_address = String::from_utf8(data[offset..offset + addr_len.0 as usize].to_vec())
            .map_err(|e| {
                Error::InvalidHandshake(format!("invalid UTF-8 in server address: {e}"))
            })?;
        offset += addr_len.0 as usize;

        // Server port (u16, big-endian)
        if offset + 2 > data.len() {
            return Err(Error::InvalidHandshake("missing server port".into()));
        }
        let server_port = u16::from_be_bytes([data[offset], data[offset + 1]]);
        offset += 2;

        // Next state
        let (next, _) = VarInt::decode(&data[offset..])?;

        let next_state = match next.0 {
            1 => NextState::Status,
            2 => NextState::Login,
            other => {
                return Err(Error::InvalidHandshake(format!(
                    "unknown next state: {other}"
                )));
            }
        };

        Ok(Some(Self {
            protocol_version: protocol_version.0,
            server_address,
            server_port,
            next_state,
        }))
    }

    /// Encode this handshake back into bytes (including the length prefix).
    pub fn to_bytes(&self) -> Result<BytesMut, Error> {
        // Craft the packet body: packet ID + fields
        let packet_id = VarInt(0x00);
        let proto = VarInt(self.protocol_version);
        let addr_len = VarInt(self.server_address.len() as i32);
        let next = VarInt(self.next_state as i32);

        let body_len = packet_id.encoded_size()
            + proto.encoded_size()
            + addr_len.encoded_size()
            + self.server_address.len()
            + 2 // port
            + next.encoded_size();

        let total_len = VarInt(body_len as i32).encoded_size() + body_len;

        let mut buf = BytesMut::with_capacity(total_len);

        // Write length prefix
        let mut tmp = [0u8; 5];
        let n = VarInt(body_len as i32).encode(&mut tmp).unwrap();
        buf.extend_from_slice(&tmp[..n]);

        // Write packet ID
        let n = packet_id.encode(&mut tmp).unwrap();
        buf.extend_from_slice(&tmp[..n]);

        // Write protocol version
        let n = proto.encode(&mut tmp).unwrap();
        buf.extend_from_slice(&tmp[..n]);

        // Write server address
        let n = addr_len.encode(&mut tmp).unwrap();
        buf.extend_from_slice(&tmp[..n]);
        buf.extend_from_slice(self.server_address.as_bytes());

        // Write port
        buf.extend_from_slice(&self.server_port.to_be_bytes());

        // Write next state
        let n = next.encode(&mut tmp).unwrap();
        buf.extend_from_slice(&tmp[..n]);

        Ok(buf)
    }
}

// MinecraftStream

/// A stream wrapper that sniffs the Minecraft handshake from the first bytes.
///
/// # Depth
/// Wraps any `AsyncRead + AsyncWrite` stream. Callers call
/// [`MinecraftStream::sniff_handshake`] once, then use the stream normally
/// (reads/writes pass through). The handshake bytes are **not consumed**
/// from the stream — they are buffered internally and replayed on the first read.
///
/// # Example
/// ```ignore
/// let mut stream = MinecraftStream::new(client);
/// if let Some(hs) = stream.sniff_handshake().await? {
///     log::info!("Client wants to connect to {}", hs.server_address);
/// }
/// // Use stream normally — reads see the handshake bytes first.
/// ```
pub struct MinecraftStream<T> {
    inner: T,
    /// Buffered bytes that the sniffer peeked but haven't been read yet.
    peek_buf: BytesMut,
    /// The sniffed handshake, if any.
    handshake: Option<Handshake>,
    /// Whether sniffing has been attempted.
    sniffed: bool,
}

impl<T: AsyncRead + AsyncWrite + Unpin> MinecraftStream<T> {
    /// Wrap a stream with Minecraft sniffing.
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            peek_buf: BytesMut::new(),
            handshake: None,
            sniffed: false,
        }
    }

    /// Try to sniff a Minecraft handshake from the stream.
    ///
    /// Reads the minimum bytes needed to decode the handshake packet
    /// (or determines the stream is not Minecraft). The peeked bytes
    /// are buffered internally and replayed on subsequent reads.
    ///
    /// Returns `None` if the stream is not a Minecraft handshake
    /// (e.g., it's HTTP or some other protocol).
    pub async fn sniff_handshake(&mut self) -> Result<Option<&Handshake>, Error> {
        if self.sniffed {
            return Ok(self.handshake.as_ref());
        }
        self.sniffed = true;

        // Read enough bytes to get the length prefix and packet ID.
        // A Minecraft handshake is typically < 512 bytes.
        let mut buf = [0u8; 512];
        let n = smol::io::AsyncReadExt::read(&mut self.inner, &mut buf).await?;

        if n == 0 {
            return Err(Error::ConnectionClosed);
        }

        // Store the peeked bytes so they can be re-read.
        self.peek_buf.extend_from_slice(&buf[..n]);

        // Try to parse the handshake from the buffered bytes.
        // First byte is the packet length VarInt — skip it for parsing body.
        let (_length, consumed) = VarInt::decode(&buf[..n])?;
        let body = &buf[consumed..n];
        self.handshake = Handshake::from_bytes(body)?;

        Ok(self.handshake.as_ref())
    }

    /// Perform a full sniff including player name and UUID extraction.
    ///
    /// This reads the handshake packet and, for login connections, the
    /// Login Start packet. All consumed bytes are buffered for replay.
    ///
    /// Returns the full [`MinecraftMetadata`] if successful.
    pub async fn sniff_full(&mut self) -> Result<MinecraftMetadata, Error> {
        if self.sniffed {
            // If we already did basic sniffing, we need to reconstruct metadata from it
            // and then read any additional packets if needed.
            if let Some(ref hs) = self.handshake {
                let mut meta = MinecraftMetadata::default();
                meta.protocol_version = hs.protocol_version;
                meta.origin_destination.clone_from(&hs.server_address);
                meta.origin_port = hs.server_port;
                meta.next_state = match hs.next_state {
                    NextState::Status => 1,
                    NextState::Login => 2,
                };
                meta.sniff_position = self.peek_buf.len();

                // If login and we haven't read the login start packet yet,
                // we need to read it now.
                if meta.next_state == 2 {
                    sniff::sniff_client_handshake(&mut self.inner, &mut self.peek_buf).await?;
                }
                return Ok(meta);
            }
            return Err(Error::Protocol(sniff::ERR_BAD_PACKET.into()));
        }

        self.sniffed = true;
        let meta = sniff::sniff_client_handshake(&mut self.inner, &mut self.peek_buf).await?;

        // Also populate the legacy Handshake field for backward compatibility
        if meta.valid() {
            self.handshake = Some(Handshake {
                protocol_version: meta.protocol_version,
                server_address: meta.origin_destination.clone(),
                server_port: meta.origin_port,
                next_state: match meta.next_state {
                    1 => NextState::Status,
                    2 => NextState::Login,
                    _ => NextState::Login,
                },
            });
        }

        Ok(meta)
    }

    /// Returns a reference to the sniffed handshake, if any.
    pub fn handshake(&self) -> Option<&Handshake> {
        self.handshake.as_ref()
    }

    /// Returns a mutable reference to the sniffed handshake, if any.
    /// Use this to modify the handshake before relaying (e.g., rewrite server address).
    pub fn handshake_mut(&mut self) -> Option<&mut Handshake> {
        self.handshake.as_mut()
    }

    /// Consume the wrapper and return the inner stream.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Returns a reference to the inner stream.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Returns a mutable reference to the inner stream.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Returns a reference to the peek buffer.
    pub fn peek_buf(&self) -> &BytesMut {
        &self.peek_buf
    }

    /// Returns a mutable reference to the peek buffer.
    pub fn peek_buf_mut(&mut self) -> &mut BytesMut {
        &mut self.peek_buf
    }

    /// Consume `n` bytes from the front of the peek buffer.
    ///
    /// Use this to skip past the handshake (or any other prefix)
    /// before relaying, so the sniffed bytes are not replayed twice.
    pub fn consume_peek(&mut self, n: usize) {
        let n = n.min(self.peek_buf.len());
        self.peek_buf.advance(n);
    }

    /// Returns the post-handshake bytes from the peek buffer.
    ///
    /// After sniffing, the peek buffer contains the handshake packet
    /// followed by any subsequent packets (status request or login start).
    /// Given the `sniff_position` (length of the handshake packet in bytes),
    /// this returns everything after it.
    pub fn post_handshake_bytes(&self, sniff_position: usize) -> &[u8] {
        &self.peek_buf[sniff_position.min(self.peek_buf.len())..]
    }
}

// Pass-through AsyncRead — replays peeked bytes first, then delegates.
impl<T: AsyncRead + AsyncWrite + Unpin> AsyncRead for MinecraftStream<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();

        // If we have peeked bytes, serve from there first.
        if !this.peek_buf.is_empty() {
            let n = std::cmp::min(buf.len(), this.peek_buf.len());
            buf[..n].copy_from_slice(&this.peek_buf[..n]);
            this.peek_buf.advance(n);
            return Poll::Ready(Ok(n));
        }

        // Delegate to inner.
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

// Pass-through AsyncWrite.
impl<T: AsyncRead + AsyncWrite + Unpin> AsyncWrite for MinecraftStream<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        Pin::new(&mut this.inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.inner).poll_close(cx)
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    /// Create raw handshake bytes for testing.
    ///
    /// Minecraft handshake packet structure:
    /// - Length (VarInt)
    /// - Packet ID (VarInt, 0x00)
    /// - Protocol version (VarInt)
    /// - Server address (VarInt length-prefixed UTF-8 string)
    /// - Server port (u16 big-endian)
    /// - Next state (VarInt)
    fn encode_handshake_raw(
        protocol_version: i32,
        server_address: &str,
        server_port: u16,
        next_state: i32,
    ) -> Vec<u8> {
        let packet_id = VarInt(0x00);
        let proto = VarInt(protocol_version);
        let addr = server_address.as_bytes();
        let addr_len = VarInt(addr.len() as i32);
        let next = VarInt(next_state);

        // Calculate body size
        let body_size = packet_id.encoded_size()
            + proto.encoded_size()
            + addr_len.encoded_size()
            + addr.len()
            + 2 // port
            + next.encoded_size();

        let mut body = Vec::with_capacity(body_size);
        let mut tmp = [0u8; 5];

        let n = packet_id.encode(&mut tmp).unwrap();
        body.extend_from_slice(&tmp[..n]);

        let n = proto.encode(&mut tmp).unwrap();
        body.extend_from_slice(&tmp[..n]);

        let n = addr_len.encode(&mut tmp).unwrap();
        body.extend_from_slice(&tmp[..n]);
        body.extend_from_slice(addr);

        body.extend_from_slice(&server_port.to_be_bytes());

        let n = next.encode(&mut tmp).unwrap();
        body.extend_from_slice(&tmp[..n]);

        // Prepend length prefix
        let total_len = VarInt(body.len() as i32);
        let mut packet = Vec::with_capacity(total_len.encoded_size() + body.len());
        let n = total_len.encode(&mut tmp).unwrap();
        packet.extend_from_slice(&tmp[..n]);
        packet.extend_from_slice(&body);

        packet
    }

    #[test]
    fn handshake_parse_hypixel() {
        let raw = encode_handshake_raw(763, "hypixel.net", 25565, 2);
        let (_length, consumed) = VarInt::decode(&raw).unwrap();
        let hs = Handshake::from_bytes(&raw[consumed..])
            .unwrap()
            .expect("should parse as handshake");

        assert_eq!(hs.protocol_version, 763);
        assert_eq!(hs.server_address, "hypixel.net");
        assert_eq!(hs.server_port, 25565);
        assert_eq!(hs.next_state, NextState::Login);
    }

    #[test]
    fn handshake_parse_status() {
        let raw = encode_handshake_raw(47, "mc.example.com", 25565, 1);
        let (_length, consumed) = VarInt::decode(&raw).unwrap();
        let hs = Handshake::from_bytes(&raw[consumed..])
            .unwrap()
            .expect("should parse as handshake");

        assert_eq!(hs.protocol_version, 47);
        assert_eq!(hs.server_address, "mc.example.com");
        assert_eq!(hs.next_state, NextState::Status);
    }

    #[test]
    fn handshake_roundtrip_encode() {
        let hs = Handshake {
            protocol_version: 763,
            server_address: "hypixel.net".to_string(),
            server_port: 25565,
            next_state: NextState::Login,
        };

        let encoded = hs.to_bytes().unwrap();
        let (_length, consumed) = VarInt::decode(&encoded).unwrap();
        let decoded = Handshake::from_bytes(&encoded[consumed..])
            .unwrap()
            .expect("should decode");

        assert_eq!(decoded, hs);
    }

    #[test]
    fn non_handshake_packet_returns_none() {
        // Packet ID 0x01 (not a handshake)
        let mut raw = encode_handshake_raw(763, "hypixel.net", 25565, 2);
        raw[1] = 0x01; // change packet ID from 0x00 to 0x01
        let (_length, consumed) = VarInt::decode(&raw).unwrap();
        let result = Handshake::from_bytes(&raw[consumed..]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn minecraft_stream_sniff() {
        smol::block_on(async {
            let raw = encode_handshake_raw(763, "hypixel.net", 25565, 2);
            // Use smol::io::Cursor which implements AsyncRead + AsyncWrite
            let cursor = smol::io::Cursor::new(raw.clone());
            let mut stream = MinecraftStream::new(cursor);

            let hs = stream.sniff_handshake().await.unwrap();
            assert!(hs.is_some());
            assert_eq!(hs.unwrap().server_address, "hypixel.net");

            // After sniffing, the peeked bytes should be replayable
            let mut output = Vec::new();
            smol::io::AsyncReadExt::read_to_end(&mut stream, &mut output)
                .await
                .unwrap();
            assert_eq!(output.len(), raw.len());
            assert_eq!(&output[..raw.len()], &raw[..]);
        });
    }

    #[test]
    fn minecraft_stream_sniff_full_status() {
        smol::block_on(async {
            let raw = encode_handshake_raw(763, "hypixel.net", 25565, 1);
            let status_req = [0x01, 0x00];
            let mut data = raw.clone();
            data.extend_from_slice(&status_req);

            let cursor = smol::io::Cursor::new(data);
            let mut stream = MinecraftStream::new(cursor);

            let meta = stream.sniff_full().await.unwrap();
            assert_eq!(meta.protocol_version, 763);
            assert_eq!(meta.origin_destination, "hypixel.net");
            assert_eq!(meta.next_state, 1);
            assert!(meta.valid());
        });
    }

    #[test]
    fn minecraft_stream_sniff_full_login() {
        smol::block_on(async {
            let raw = encode_handshake_raw(764, "mc.example.com", 25565, 2);
            let mut login_body = BytesMut::new();
            packet::write_byte(&mut login_body, 0x00); // packet ID
            packet::write_string(&mut login_body, "TestPlayer");
            let uuid: [u8; 16] = [0x01; 16];
            login_body.extend_from_slice(&uuid);

            let mut login_packet = BytesMut::new();
            let mut tmp = [0u8; 5];
            let n = VarInt(login_body.len() as i32).encode(&mut tmp).unwrap();
            login_packet.extend_from_slice(&tmp[..n]);
            login_packet.extend_from_slice(&login_body);

            let mut data = raw.clone();
            data.extend_from_slice(&login_packet);

            let cursor = smol::io::Cursor::new(data);
            let mut stream = MinecraftStream::new(cursor);

            let meta = stream.sniff_full().await.unwrap();
            assert_eq!(meta.protocol_version, 764);
            assert_eq!(meta.player_name, "TestPlayer");
            assert_eq!(meta.next_state, 2);
            assert!(meta.valid());
        });
    }
}
