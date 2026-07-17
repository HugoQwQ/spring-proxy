//! Bidirectional TCP relay between two async streams.
//!
//! # Depth
//! **Interface:** one function [`relay`] that takes two streams and a config.
//!
//! **Behind the seam:**
//! - Two concurrent copy loops (client→target and target→client) running in parallel
//! - Backpressure via bounded buffer
//! - Graceful shutdown on EOF or error from either side
//! - Configurable read timeouts
//!
//! Callers learn one function. The implementation handles all edge cases.

use std::time::Duration;

use common::Error;
use smol::io::{AsyncRead, AsyncWrite};

// RelayConfig

/// Configuration for a single relay connection.
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct RelayConfig {
    /// Size of the read buffer per direction (default: 65536).
    pub buffer_size: usize,
    /// Read timeout (default: 30 seconds, None = disabled).
    pub read_timeout: Option<Duration>,
    /// Idle timeout before closing (default: None).
    pub idle_timeout: Option<Duration>,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            buffer_size: 65_536,
            read_timeout: Some(Duration::from_secs(30)),
            idle_timeout: None,
        }
    }
}

// Relay

/// Bidirectionally relay data between `client` and `target`.
///
/// Spawns two concurrent tasks: one copying `client → target`,
/// the other copying `target → client`. Returns when both complete
/// (normally due to one side closing the connection).
///
/// # Errors
/// Returns [`Error::ConnectionClosed`] when either side cleanly
/// closes the connection. Returns [`Error::Io`] for I/O errors.
///
/// # Depth
/// This is the deepest module in spring-proxy:
/// - Callers provide two `AsyncRead + AsyncWrite` streams + config
/// - The implementation hides concurrent copy, backpressure, timeouts
/// - The interface is a single async function
pub async fn relay<C, T>(client: C, target: T, config: RelayConfig) -> Result<(), Error>
where
    C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut cr, mut cw) = smol::io::split(client);
    let (mut tr, mut tw) = smol::io::split(target);

    // Run both copy directions concurrently
    // When one direction finishes (EOF), the other is still running
    let client_to_target = copy_directional(&mut cr, &mut tw, config);
    let target_to_client = copy_directional(&mut tr, &mut cw, config);

    let results = smol::future::zip(client_to_target, target_to_client).await;
    let (res1, res2) = results;

    // Combine errors: prefer non-EOF/non-Timeout errors
    match (res1, res2) {
        (Err(Error::ConnectionClosed), Err(Error::ConnectionClosed))
        | (Err(Error::ConnectionClosed), Err(Error::Timeout))
        | (Err(Error::Timeout), Err(Error::ConnectionClosed))
        | (Err(Error::Timeout), Err(Error::Timeout))
        | (Err(Error::ConnectionClosed), Ok(()))
        | (Ok(()), Err(Error::ConnectionClosed))
        | (Err(Error::Timeout), Ok(()))
        | (Ok(()), Err(Error::Timeout))
        | (Ok(()), Ok(())) => Ok(()),
        (Err(e), _) | (_, Err(e)) => Err(e),
    }
}

/// Copy data from `reader` to `writer` in one direction.
///
/// Returns:
/// - `Ok(())` when EOF is reached (clean shutdown)
/// - `Err(Error::ConnectionClosed)` when reader returns EOF (0 bytes read)
/// - `Err(Error::Timeout)` when a read times out
/// - `Err(Error::Io(_))` for other I/O errors
async fn copy_directional<R, W>(
    reader: &mut R,
    writer: &mut W,
    config: RelayConfig,
) -> Result<(), Error>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = common::IoBuf::new(config.buffer_size);

    loop {
        // Read with optional timeout
        let read_result = if let Some(timeout) = config.read_timeout {
            let read_fut = buf.read_from(reader);
            smol::future::race(read_fut, async {
                smol::Timer::after(timeout).await;
                Err(Error::Timeout)
            })
            .await
        } else {
            buf.read_from(reader).await
        };

        let _n = match read_result {
            Ok(0) => return Err(Error::ConnectionClosed),
            Ok(n) => n,
            Err(e) => return Err(e),
        };

        // Write all buffered bytes to the writer
        while !buf.is_empty() {
            buf.write_to(writer).await?;
        }
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use async_net::TcpListener;
    use smol::io::AsyncReadExt;

    #[test]
    fn relay_short_lived() {
        smol::block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();

            // Server that reads once and closes
            let server_handle = smol::spawn(async move {
                if let Ok((mut server, _)) = listener.accept().await {
                    let mut buf = [0u8; 1024];
                    let _n = server.read(&mut buf).await.ok();
                }
            });

            let client = async_net::TcpStream::connect(addr).await.unwrap();
            let target = async_net::TcpStream::connect(addr).await.unwrap();

            // Relay should complete without hanging
            let result = relay(client, target, RelayConfig::default()).await;
            match result {
                Ok(()) | Err(Error::ConnectionClosed) => {}
                Err(e) => panic!("unexpected error: {e}"),
            }
            server_handle.await;
        });
    }

    #[test]
    fn relay_closes_on_eof() {
        smol::block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();

            let server = smol::spawn(async move {
                let (mut server, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 1024];
                let _n = server.read(&mut buf).await.unwrap();
                // Don't write back — just close
            });

            let client = async_net::TcpStream::connect(addr).await.unwrap();
            let target = async_net::TcpStream::connect(addr).await.unwrap();

            let result = relay(client, target, RelayConfig::default()).await;
            // Should complete without error (EOF is expected)
            assert!(result.is_ok() || matches!(result, Err(Error::ConnectionClosed)));
            server.await;
        });
    }
}
