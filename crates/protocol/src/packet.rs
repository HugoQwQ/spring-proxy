//! Minecraft packet I/O utilities.
//!
//! Provides helpers for reading and writing length-prefixed Minecraft packets,
//! VarInts from async streams, strings, and big-endian integers.

use common::{Buf, BytesMut, Error, VarInt};
use smol::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Maximum possible size of a handshake packet (as enforced by ZBProxy).
pub const MAX_HANDSHAKE_PACKET_SIZE: i32 = 264;

/// Maximum length of a VarInt in bytes.
pub const MAX_VARINT_LEN: usize = VarInt::MAX_ENCODED_SIZE;

/// Read a VarInt from an async reader.
///
/// Returns the decoded value and the number of bytes consumed.
pub async fn read_varint<R: AsyncRead + Unpin>(reader: &mut R) -> Result<(i32, usize), Error> {
    let mut value = 0u32;
    for i in 0..MAX_VARINT_LEN {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte).await?;
        let b = byte[0];
        value |= ((b & 0x7F) as u32) << (i * 7);
        if (b & 0x80) == 0 {
            return Ok((value as i32, i + 1));
        }
    }
    Err(Error::VarIntTooLarge(value as i32))
}

/// Read a length-prefixed packet from an async reader.
///
/// Returns the packet body (without the length prefix) in a [`BytesMut`].
/// The packet body includes the packet ID as its first byte(s).
pub async fn read_packet<R: AsyncRead + Unpin>(reader: &mut R) -> Result<BytesMut, Error> {
    let (length, _) = read_varint(reader).await?;
    if length < 0 {
        return Err(Error::Protocol(format!(
            "incorrect packet length: {length}"
        )));
    }
    let mut buf = vec![0u8; length as usize];
    reader.read_exact(&mut buf).await?;
    Ok(BytesMut::from(&buf[..]))
}

/// Read a length-prefixed packet with a maximum size limit.
///
/// Returns an error if the packet length exceeds `max_len`.
pub async fn read_packet_limited<R: AsyncRead + Unpin>(
    reader: &mut R,
    max_len: i32,
) -> Result<BytesMut, Error> {
    let (length, _) = read_varint(reader).await?;
    if length < 0 {
        return Err(Error::Protocol(format!(
            "incorrect packet length: {length}"
        )));
    }
    if length > max_len {
        return Err(Error::Protocol(format!(
            "packet max length exceeded: length={length}, max={max_len}"
        )));
    }
    let mut buf = vec![0u8; length as usize];
    reader.read_exact(&mut buf).await?;
    Ok(BytesMut::from(&buf[..]))
}

/// Write a length-prefixed packet to an async writer.
///
/// `body` should contain the packet ID and payload (without the length prefix).
/// The length is computed from `body.len()` and prepended automatically.
pub async fn write_packet<W: AsyncWrite + Unpin>(writer: &mut W, body: &[u8]) -> Result<(), Error> {
    let length = VarInt(body.len() as i32);
    let mut prefix = [0u8; MAX_VARINT_LEN];
    let n = length.encode(&mut prefix)?;
    writer.write_all(&prefix[..n]).await?;
    writer.write_all(body).await?;
    writer.flush().await?;
    Ok(())
}

/// Append a packet length prefix to the front of a buffer.
///
/// # Panics
/// Panics if the buffer does not have enough headroom for the VarInt.
/// This is typically used with a buffer that was allocated with at least
/// `MAX_VARINT_LEN` bytes of headroom.
pub fn append_packet_length(buffer: &mut BytesMut, length: usize) {
    let len_varint = VarInt(length as i32);
    let n = len_varint.encoded_size();
    // Prepend by copying existing data forward
    buffer.reserve(n);
    let old_len = buffer.len();
    unsafe {
        buffer.set_len(old_len + n);
    }
    buffer.copy_within(0..old_len, n);
    let mut tmp = [0u8; MAX_VARINT_LEN];
    len_varint
        .encode(&mut tmp)
        .expect("5-byte buffer always sufficient");
    buffer[..n].copy_from_slice(&tmp[..n]);
}

/// Read a length-prefixed UTF-8 string from a byte buffer.
///
/// Returns an error if the string length exceeds `limit`.
pub fn read_limited_string(buffer: &mut BytesMut, limit: i32) -> Result<String, Error> {
    let (len, consumed) = VarInt::decode(buffer)?;
    let len = len.0;
    if len > limit {
        return Err(Error::Protocol("string length limit exceeded".into()));
    }
    if len < 0 {
        return Err(Error::Protocol("bad string length: negative".into()));
    }
    if len == 0 {
        return Ok(String::new());
    }
    let len = len as usize;
    if len > buffer.len() - consumed {
        return Err(Error::UnexpectedEof);
    }
    let s = String::from_utf8(buffer[consumed..consumed + len].to_vec())
        .map_err(|e| Error::Protocol(format!("invalid UTF-8 in string: {e}")))?;
    buffer.advance(consumed + len);
    Ok(s)
}

/// Read a length-prefixed UTF-8 string with no limit.
pub fn read_string(buffer: &mut BytesMut) -> Result<String, Error> {
    read_limited_string(buffer, i32::MAX)
}

/// Write a length-prefixed UTF-8 string into a buffer.
pub fn write_string(buffer: &mut BytesMut, s: &str) {
    let len = VarInt(s.len() as i32);
    let mut tmp = [0u8; MAX_VARINT_LEN];
    let n = len
        .encode(&mut tmp)
        .expect("5-byte buffer always sufficient");
    buffer.extend_from_slice(&tmp[..n]);
    buffer.extend_from_slice(s.as_bytes());
}

/// Read a `u16` in big-endian from a byte buffer.
pub fn read_u16_be(buffer: &mut BytesMut) -> Result<u16, Error> {
    if buffer.len() < 2 {
        return Err(Error::UnexpectedEof);
    }
    let val = u16::from_be_bytes([buffer[0], buffer[1]]);
    buffer.advance(2);
    Ok(val)
}

/// Write a `u16` in big-endian into a buffer.
pub fn write_u16_be(buffer: &mut BytesMut, val: u16) {
    buffer.extend_from_slice(&val.to_be_bytes());
}

/// Read a `u64` in big-endian from a byte buffer.
pub fn read_u64_be(buffer: &mut BytesMut) -> Result<u64, Error> {
    if buffer.len() < 8 {
        return Err(Error::UnexpectedEof);
    }
    let val = u64::from_be_bytes([
        buffer[0], buffer[1], buffer[2], buffer[3], buffer[4], buffer[5], buffer[6], buffer[7],
    ]);
    buffer.advance(8);
    Ok(val)
}

/// Write a `u64` in big-endian into a buffer.
pub fn write_u64_be(buffer: &mut BytesMut, val: u64) {
    buffer.extend_from_slice(&val.to_be_bytes());
}

/// Read a single byte from a byte buffer.
pub fn read_byte(buffer: &mut BytesMut) -> Result<u8, Error> {
    if buffer.is_empty() {
        return Err(Error::UnexpectedEof);
    }
    let b = buffer[0];
    buffer.advance(1);
    Ok(b)
}

/// Write a single byte into a buffer.
pub fn write_byte(buffer: &mut BytesMut, byte: u8) {
    buffer.extend_from_slice(&[byte]);
}

/// Minecraft packet intent / next state values.
pub mod intent {
    /// Status request (server list ping).
    pub const STATUS: i8 = 1;
    /// Login (join server).
    pub const LOGIN: i8 = 2;
    /// Transfer (added in 1.20.5).
    pub const TRANSFER: i8 = 3;
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip_from_bytes() {
        // Test values from wiki.vg
        let cases = [
            (0, vec![0]),
            (1, vec![1]),
            (2, vec![2]),
            (127, vec![127]),
            (128, vec![128, 1]),
            (255, vec![255, 1]),
            (25565, vec![221, 199, 1]),
            (2_097_151, vec![255, 255, 127]),
            (2_147_483_647, vec![255, 255, 255, 255, 7]),
            (-1, vec![255, 255, 255, 255, 15]),
            (-2_147_483_648, vec![128, 128, 128, 128, 8]),
        ];
        for (expected, bytes) in cases {
            let buf = BytesMut::from(&bytes[..]);
            let (val, consumed) = VarInt::decode(&buf).unwrap();
            assert_eq!(val.0, expected, "decode mismatch for {expected}");
            assert_eq!(consumed, bytes.len());
        }
    }

    #[test]
    fn read_string_basic() {
        let mut buf = BytesMut::new();
        write_string(&mut buf, "hypixel.net");
        let s = read_string(&mut buf).unwrap();
        assert_eq!(s, "hypixel.net");
        assert!(buf.is_empty());
    }

    #[test]
    fn read_limited_string_under_limit() {
        let mut buf = BytesMut::new();
        write_string(&mut buf, "test");
        let s = read_limited_string(&mut buf, 100).unwrap();
        assert_eq!(s, "test");
    }

    #[test]
    fn read_limited_string_over_limit() {
        let mut buf = BytesMut::new();
        write_string(&mut buf, "test");
        assert!(read_limited_string(&mut buf, 2).is_err());
    }

    #[test]
    fn u16_be_roundtrip() {
        let mut buf = BytesMut::new();
        write_u16_be(&mut buf, 25565);
        assert_eq!(buf.len(), 2);
        let val = read_u16_be(&mut buf).unwrap();
        assert_eq!(val, 25565);
        assert!(buf.is_empty());
    }

    #[test]
    fn append_packet_length_basic() {
        let mut buf = BytesMut::from(&[0x00, 0x01, 0x02][..]);
        append_packet_length(&mut buf, 3);
        // Should now be: [VarInt(3)] [0x00, 0x01, 0x02]
        assert_eq!(buf.len(), 4);
        let (len, consumed) = VarInt::decode(&buf).unwrap();
        assert_eq!(len.0, 3);
        assert_eq!(&buf[consumed..], &[0x00, 0x01, 0x02]);
    }
}
