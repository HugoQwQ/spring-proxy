# Architecture

Spring Proxy is a Rust workspace with three crates. Each crate is designed as a **deep module**: a small public interface hiding complex implementation.

## Crate layout

```
┌─────────────────────────────────────────────────────────────┐
│                         spring                               │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐        │
│  │ service │─▶│ router  │─▶│ outbound│─▶│  relay  │        │
│  │(listener│  │(rules)  │  │(target) │  │(bidir)  │        │
│  └─────────┘  └─────────┘  └─────────┘  └─────────┘        │
│       ▲                                        │            │
│       └────────────────────────────────────────┘            │
│                          (TCP loopback)                     │
└─────────────────────────────────────────────────────────────┘
                              │
                    ┌─────────┴─────────┐
                    ▼                   ▼
              ┌──────────┐      ┌──────────┐
              │ protocol │      │  common  │
              │(Minecraft│      │(VarInt,  │
              │ packets) │      │ errors)  │
              └──────────┘      └──────────┘
```

### `common`

Shared vocabulary for the whole project.

- **`VarInt`** — Minecraft variable-length integer encode/decode
- **`Error`** — unified error enum (I/O, protocol, routing, timeout)
- **`IoBuf`** — reusable byte buffer for relay copies
- **`access`** — allow/block list checking
- **`set`** — string sets with JSON (de)serialization
- **`domain`** — exact and suffix domain matching

### `protocol`

Everything Minecraft-specific.

- **`Handshake`** / **`NextState`** — basic handshake packet structure
- **`MinecraftStream`** — `AsyncRead + AsyncWrite` wrapper that peeks the handshake and buffers bytes for replay
- **`packet`** — low-level packet I/O (VarInt length prefixes, strings, bytes)
- **`sniff`** — async handshake + login start packet extraction
- **`metadata`** — `MinecraftMetadata` with FML detection
- **`motd`** — status response JSON generation
- **`kick`** — disconnect message generation
- **`message`** — Minecraft chat component JSON format

### `spring`

The application layer.

- **`service`** — TCP listener, IP access control, sniff-then-dispatch
- **`route`** — rule chain evaluation, built-in outbounds (REJECT, RESET)
- **`outbound`** — `PlainOutbound` and `MinecraftOutbound`
- **`relay`** — bidirectional async TCP copy with timeout support
- **`stream`** — `Stream` enum unifying plain TCP and `MinecraftStream`
- **`runner`** — orchestrator: outbounds → router → services
- **`config`** — TOML configuration structs

## Connection flow

```
Client ──TCP──▶ Service::accept()
                     │
                     ▼
              IP access control?
                     │
                     ▼
              MinecraftStream::sniff_full()
                     │
                     ▼
              ConnectionMetadata
                     │
                     ▼
              Router::handle_connection()
                     │
                     ▼
              Rule evaluation
                     │
                     ▼
              Outbound::handle_connection()
                     │
           ┌─────────┴─────────┐
           ▼                   ▼
    PlainOutbound      MinecraftOutbound
           │                   │
           ▼                   ▼
    async_net::connect   async_net::connect
           │                   │
           ▼                   ▼
      relay::relay()     Status → MOTD
                         Login  → rewrite → relay
```

### Key design decisions

1. **Sniff upfront** — The service always attempts `sniff_full()` before dispatching. If it fails, the connection unwraps to a plain TCP stream. This means partial reads never corrupt non-Minecraft traffic.

2. **Replay buffer** — `MinecraftStream` buffers all bytes read during sniffing. When the outbound forwards to the backend, it sends a rewritten handshake plus the original post-handshake bytes (status request or login start). Then it consumes the peek buffer so `relay` doesn't send them again.

3. **Stream enum** — Rather than trait objects (`Box<dyn AsyncRead + AsyncWrite>`), we use a concrete `Stream` enum. This avoids object-safety issues, dynamic dispatch, and accidental unsoundness. The enum is `Unpin`, so `smol::io::split` works natively.

4. **Rule chain** — Rules are evaluated in order, first match wins. Each rule can sniff, rewrite, and select an outbound. This is simple, predictable, and fast.

## Stream handling

The `Stream` enum is the seam between service and outbound:

```rust
pub enum Stream {
    Plain(async_net::TcpStream),
    Minecraft(Box<protocol::MinecraftStream<async_net::TcpStream>>),
}
```

### Why an enum?

- **Type safety** — Callers know exactly what they have
- **No vtable** — Zero-cost abstraction
- **`Unpin`** — Works with `smol::io::split` for relay
- **Peek operations** — `consume_peek`, `post_handshake_bytes` only exist for the Minecraft variant

### Consumption pattern

After the outbound sends the rewritten handshake to the backend, it must consume the corresponding bytes from the peek buffer:

```rust
// Send rewritten handshake + original post-handshake bytes
target.write_all(&packet_buf).await?;

// CRITICAL: consume ALL peeked bytes so relay doesn't replay them
stream.consume_peek(usize::MAX);

// Now relay only copies NEW bytes from client ↔ backend
relay::relay(stream, target, RelayConfig::default()).await?;
```

Forgetting this step causes the backend to receive duplicate packets (e.g., Login Start twice), which confuses the Minecraft protocol state machine and leads to immediate disconnects.
