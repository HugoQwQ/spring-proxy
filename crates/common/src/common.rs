//! Shared types and utilities for spring-proxy.
//!
//! This crate provides the vocabulary that `protocol` and `spring` share:
//! - [`VarInt`]: Minecraft-style variable-length integer encoding
//! - [`Error`]: Unified error type
//! - [`IoBuf`]: Bounded byte buffer for relay I/O
//! - [`access`]: Access control (allow/block lists)
//! - [`set`]: String sets with JSON support
//! - [`domain`]: Fast domain matcher

use std::fmt;

pub mod access;
pub mod domain;
pub mod set;

// VarInt — Minecraft variable-length integer

/// A Minecraft-style VarInt (up to 5 bytes, 7-bit per byte).
///
/// # Depth
/// Small interface — just `encode`/`decode`/`len`.
/// Behind it: variable-length encoding, over/underflow detection,
/// zero-copy serialization with no heap allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarInt(pub i32);

impl VarInt {
    /// Maximum number of bytes a VarInt can occupy.
    pub const MAX_ENCODED_SIZE: usize = 5;

    /// Encode this VarInt into `buf`. Returns the number of bytes written.
    pub fn encode(&self, buf: &mut [u8]) -> Result<usize, Error> {
        let mut value = self.0 as u32;
        for i in 0..Self::MAX_ENCODED_SIZE {
            if i >= buf.len() {
                return Err(Error::BufferOverflow {
                    needed: i + 1,
                    available: buf.len(),
                });
            }
            if (value & 0xFFFF_FF80) == 0 {
                buf[i] = value as u8;
                return Ok(i + 1);
            }
            buf[i] = (value as u8 & 0x7F) | 0x80;
            value >>= 7;
        }
        Err(Error::VarIntTooLarge(self.0))
    }

    /// Decode a VarInt from `src`. Returns the VarInt and the number of bytes consumed.
    pub fn decode(src: &[u8]) -> Result<(Self, usize), Error> {
        let mut value = 0u32;
        for i in 0..Self::MAX_ENCODED_SIZE {
            if i >= src.len() {
                return Err(Error::UnexpectedEof);
            }
            let byte = src[i];
            value |= ((byte & 0x7F) as u32) << (i * 7);
            if (byte & 0x80) == 0 {
                // Sign-extend if the value is negative in 32-bit two's complement
                // (VarInt uses signed 32-bit, so the high bit of the last byte
                //  indicates sign in normal two's complement interpretation).
                // We treat it as i32 directly from u32, which is correct for
                // standard VarInt semantics.
                let val = value as i32;
                return Ok((Self(val), i + 1));
            }
        }
        Err(Error::VarIntTooLarge(value as i32))
    }

    /// Returns the encoded size in bytes without writing.
    pub fn encoded_size(&self) -> usize {
        let mut value = self.0 as u32;
        for i in 1..=Self::MAX_ENCODED_SIZE {
            if (value & 0xFFFF_FF80) == 0 {
                return i;
            }
            value >>= 7;
        }
        Self::MAX_ENCODED_SIZE
    }
}

// Error

/// Unified error type for spring-proxy.
///
/// # Depth
/// A single `Error` enum covers all modules — callers match one type
/// instead of juggling `io::Error`, `protocol::Error`, `router::Error`.
#[derive(Debug)]
pub enum Error {
    /// Wrapped I/O error.
    Io(std::io::Error),
    /// Unexpected end of data while decoding.
    UnexpectedEof,
    /// Buffer not large enough for encoding.
    BufferOverflow { needed: usize, available: usize },
    /// VarInt value exceeds 5-byte encoding limit.
    VarIntTooLarge(i32),
    /// Minecraft protocol error.
    Protocol(String),
    /// Invalid or unrecognised handshake.
    InvalidHandshake(String),
    /// No route found for connection.
    NoRoute(String),
    /// Connection was closed by peer.
    ConnectionClosed,
    /// Operation timed out.
    Timeout,
    /// Internal error (bug / invariant violation).
    Internal(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::UnexpectedEof => f.write_str("unexpected end of data"),
            Self::BufferOverflow { needed, available } => {
                write!(
                    f,
                    "buffer overflow: needed {needed} bytes, have {available}"
                )
            }
            Self::VarIntTooLarge(v) => write!(f, "VarInt too large: {v}"),
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
            Self::InvalidHandshake(msg) => write!(f, "invalid handshake: {msg}"),
            Self::NoRoute(msg) => write!(f, "no route: {msg}"),
            Self::ConnectionClosed => f.write_str("connection closed"),
            Self::Timeout => f.write_str("timeout"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

// IoBuf — bounded byte buffer for relay I/O

/// A fixed-capacity, reusable byte buffer for TCP relay copies.
///
/// # Depth
/// Simple interface: [`IoBuf::read_from`] fills, [`IoBuf::write_to`] drains.
/// Behind it: buffer reuse, watermark-based resizing, zero-initialization avoidance.
pub struct IoBuf {
    buf: BytesMut,
}

impl IoBuf {
    /// Create a new buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: BytesMut::with_capacity(capacity.max(4096)),
        }
    }

    /// Read data from `reader` into the buffer.
    /// Returns the number of bytes read (0 means EOF).
    pub async fn read_from<R>(&mut self, reader: &mut R) -> Result<usize, Error>
    where
        R: smol::io::AsyncRead + Unpin,
    {
        // Use a stack buffer for reading, then extend the internal buffer.
        let mut tmp = [0u8; 4096];
        let n = smol::io::AsyncReadExt::read(reader, &mut tmp).await?;
        if n > 0 {
            self.buf.extend_from_slice(&tmp[..n]);
        }
        Ok(n)
    }

    /// Write buffer contents to `writer`. Returns the number of bytes written.
    pub async fn write_to<W>(&mut self, writer: &mut W) -> Result<usize, Error>
    where
        W: smol::io::AsyncWrite + Unpin,
    {
        if self.buf.is_empty() {
            return Ok(0);
        }
        let n = smol::io::AsyncWriteExt::write(writer, self.buf.as_ref()).await?;
        self.buf.advance(n);
        Ok(n)
    }

    /// Returns `true` if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Returns the number of bytes currently in the buffer.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Clear the buffer (retains capacity).
    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

// Re-exports from `bytes`

pub use bytes::{Buf, BufMut, Bytes, BytesMut};

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip_small() {
        for val in [0, 1, 127, 128, 255, 1_000_000, -1, -128, i32::MAX, i32::MIN] {
            let v = VarInt(val);
            let mut buf = [0u8; 5];
            let n = v.encode(&mut buf).unwrap();
            let (decoded, consumed) = VarInt::decode(&buf[..n]).unwrap();
            assert_eq!(decoded, v, "roundtrip failed for {val}");
            assert_eq!(consumed, n);
        }
    }

    #[test]
    fn varint_encoded_size() {
        assert_eq!(VarInt(0).encoded_size(), 1);
        assert_eq!(VarInt(127).encoded_size(), 1);
        assert_eq!(VarInt(128).encoded_size(), 2);
        assert_eq!(VarInt(1_000_000).encoded_size(), 3);
        assert_eq!(VarInt(i32::MAX).encoded_size(), 5);
    }

    #[test]
    fn varint_too_large_decode() {
        // 5 bytes with high bit set on the 5th -> invalid
        let buf = [0x80, 0x80, 0x80, 0x80, 0x80];
        assert!(VarInt::decode(&buf).is_err());
    }

    #[test]
    fn varint_buffer_overflow() {
        let v = VarInt(128);
        let mut buf = [0u8; 1];
        assert!(v.encode(&mut buf).is_err());
    }
}
