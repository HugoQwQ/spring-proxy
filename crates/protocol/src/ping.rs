//! Minecraft ping mode constants.
//!
//! Defines how the proxy should respond to server-list ping (status) requests.

/// Disconnect immediately after sending the MOTD (no ping response).
pub const PING_MODE_DISCONNECT: &str = "disconnect";

/// Respond to the ping request with a 0 ms latency (no round-trip to backend).
pub const PING_MODE_0MS: &str = "0ms";

/// Default ping mode: proxy the ping request through to the backend server.
pub const PING_MODE_DEFAULT: &str = "default";
