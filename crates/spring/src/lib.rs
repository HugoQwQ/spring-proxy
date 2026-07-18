//! Spring Proxy — high-performance multipurpose TCP relay.
//!
//! # Depth
//! This crate exposes deep modules:
//!
//! 1. [`relay`] — bidirectional TCP copy between two streams.
//!    Interface: 1 function. Behind it: zero-copy relay, backpressure, graceful shutdown.
//!
//! 2. [`route`] — routing rules that map connection metadata to outbounds.
//!    Interface: 1 method (`Router::handle_connection`). Behind it: rule chain evaluation,
//!    wildcard/pattern matching, default route fallback, sniffing, rewriting.
//!
//! 3. [`outbound`] — outbound implementations (plain TCP relay, Minecraft proxy).
//!
//! 4. [`service`] — TCP listener that accepts connections and dispatches them.
//!
//! 5. [`Runner`] — the top-level proxy orchestrator.
//!    Interface: 1 method (`Runner::run`). Behind it: initialize outbounds → router → services.

#![allow(clippy::style)]

pub(crate) mod config;
pub mod outbound;
pub mod relay;
pub mod route;
pub mod runner;
pub mod service;
pub mod stream;

pub use outbound::Outbound;
pub use relay::relay;
pub use route::Router;
pub use runner::Runner;
