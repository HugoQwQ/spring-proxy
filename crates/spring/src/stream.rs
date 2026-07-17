//! Stream types for Spring Proxy.
//!
//! Provides a unified async stream that can wrap either a plain
//! TCP stream or a sniffing-aware [`MinecraftStream`].

use std::pin::Pin;
use std::task::{Context, Poll};

use smol::io::{AsyncRead, AsyncWrite};

/// A unified async stream for inbound connections.
///
/// Either a plain TCP stream or a [`MinecraftStream`] that has already
/// sniffed the handshake and buffers bytes for replay.
pub enum Stream {
    Plain(async_net::TcpStream),
    Minecraft(Box<protocol::MinecraftStream<async_net::TcpStream>>),
}

impl Stream {
    /// Wrap a plain TCP stream.
    pub fn plain(stream: async_net::TcpStream) -> Self {
        Self::Plain(stream)
    }

    /// Wrap a Minecraft sniffing stream.
    pub fn minecraft(stream: protocol::MinecraftStream<async_net::TcpStream>) -> Self {
        Self::Minecraft(Box::new(stream))
    }

    /// Returns a reference to the sniffed handshake, if this is a Minecraft stream.
    pub fn handshake(&self) -> Option<&protocol::Handshake> {
        match self {
            Self::Minecraft(mc) => mc.handshake(),
            Self::Plain(_) => None,
        }
    }

    /// Returns a mutable reference to the sniffed handshake.
    pub fn handshake_mut(&mut self) -> Option<&mut protocol::Handshake> {
        match self {
            Self::Minecraft(mc) => mc.handshake_mut(),
            Self::Plain(_) => None,
        }
    }

    /// Try to get a reference to the inner Minecraft stream.
    pub fn as_minecraft(&self) -> Option<&protocol::MinecraftStream<async_net::TcpStream>> {
        match self {
            Self::Minecraft(mc) => Some(mc),
            Self::Plain(_) => None,
        }
    }

    /// Try to get a mutable reference to the inner Minecraft stream.
    pub fn as_minecraft_mut(
        &mut self,
    ) -> Option<&mut protocol::MinecraftStream<async_net::TcpStream>> {
        match self {
            Self::Minecraft(mc) => Some(mc),
            Self::Plain(_) => None,
        }
    }

    /// Unwrap into a plain TCP stream, or return `Err(self)` if it's a Minecraft stream.
    pub fn into_plain(self) -> Result<async_net::TcpStream, Self> {
        match self {
            Self::Plain(tcp) => Ok(tcp),
            Self::Minecraft(_) => Err(self),
        }
    }

    /// Unwrap into a Minecraft stream, or return `Err(self)` if it's plain.
    pub fn into_minecraft(
        self,
    ) -> Result<Box<protocol::MinecraftStream<async_net::TcpStream>>, Self> {
        match self {
            Self::Minecraft(mc) => Ok(mc),
            Self::Plain(_) => Err(self),
        }
    }

    /// Convert into a plain TCP stream.
    ///
    /// For a Minecraft stream, this extracts the inner stream (losing the
    /// sniffed handshake buffer — use with care).
    pub fn into_inner_tcp(self) -> async_net::TcpStream {
        match self {
            Self::Plain(tcp) => tcp,
            Self::Minecraft(mc) => mc.into_inner(),
        }
    }

    /// Returns the post-handshake bytes from a Minecraft stream.
    pub fn post_handshake_bytes(&self, sniff_position: usize) -> Option<&[u8]> {
        match self {
            Self::Minecraft(mc) => Some(mc.post_handshake_bytes(sniff_position)),
            Self::Plain(_) => None,
        }
    }

    /// Consume `n` bytes from the front of a Minecraft stream's peek buffer.
    pub fn consume_peek(&mut self, n: usize) {
        if let Self::Minecraft(mc) = self {
            mc.consume_peek(n);
        }
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(tcp) => Pin::new(tcp).poll_read(cx, buf),
            Self::Minecraft(mc) => Pin::new(mc).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(tcp) => Pin::new(tcp).poll_write(cx, buf),
            Self::Minecraft(mc) => Pin::new(mc).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(tcp) => Pin::new(tcp).poll_flush(cx),
            Self::Minecraft(mc) => Pin::new(mc).poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(tcp) => Pin::new(tcp).poll_close(cx),
            Self::Minecraft(mc) => Pin::new(mc).poll_close(cx),
        }
    }
}
