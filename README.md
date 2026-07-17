# Spring Proxy

A high-performance TCP relay for Minecraft, inspired by [ZBProxy](https://github.com/layou233/ZBProxy).

## Features

- **Minecraft handshake sniffing** — inspects the first packet to extract protocol version, target hostname, player name, and UUID
- **Rule-based routing** — route connections based on service name, source IP/port, Minecraft hostname, player name, and more
- **Minecraft outbound proxy** — full Minecraft proxy with:
  - Custom MOTD (Message of the Day) responses
  - Player name access control
  - Online player count limiting
  - Hostname rewriting
  - Forge Mod Loader (FML) suffix handling
- **Plain TCP relay** — transparent bidirectional relay for any TCP traffic
- **Graceful shutdown** — waits for active connections to finish on SIGINT

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Service   │────▶│   Router    │────▶│  Outbound   │
│  (listener) │     │ (rule chain)│     │(Minecraft/  │
└─────────────┘     └─────────────┘     │   Plain)    │
                                        └─────────────┘
```

- **`common`** — shared utilities: VarInt encoding, error types, access control, domain matching
- **`protocol`** — Minecraft protocol: packet I/O, chat messages, MOTD generation, handshake sniffing
- **`spring`** — application layer: relay, routing, outbounds, services, configuration

## Quick Start

```bash
# Build
cargo build --release

# Run (creates spring.toml with defaults if missing)
./target/release/spring

# Or specify a config file
./target/release/spring --config my-config.toml
```

## Configuration

Spring Proxy uses TOML configuration. On first run, it generates a default `spring.toml`:

```toml
log.level = "info"

[[services]]
name = "Hypixel-in"
listen = 25565

[router]
default_outbound = "RESET"

[[router.rules]]
type = "always"
sniff = ["minecraft"]

[[router.rules]]
type = "ServiceName"
parameter = "Hypixel-in"
outbound = "Hypixel-out"

[router.rules.rewrite.minecraft]
hostname = "mc.hypixel.net"
port = 25565

[[outbounds]]
name = "Hypixel-out"
target_address = "mc.hypixel.net"
target_port = 25565

[outbounds.minecraft]
motd_favicon = "{DEFAULT_MOTD}"
motd_description = "§d{NAME}§e, provided by §a§o{INFO}§r\n§c§lProxy for §6§n{HOST}:{PORT}§r"
```

### Rule types

| Type | Description |
|------|-------------|
| `always` | Always matches |
| `and` | All sub-rules must match |
| `or` | Any sub-rule must match |
| `ServiceName` | Match by inbound service name |
| `SourceIPVersion` | Match by IP version (`4` or `6`) |
| `SourceIP` | Match by source IP/CIDR |
| `SourcePort` | Match by source port |
| `MinecraftHostname` | Match by sniffed Minecraft hostname |
| `MinecraftPlayerName` | Match by sniffed player name |
| `MinecraftStatus` | Match if next state is status |
| `MinecraftTransfer` | Match if next state is transfer |

### Ping modes

- `"disconnect"` — send MOTD then close immediately
- `"0ms"` — respond with 0ms latency
- `"default"` (or any other) — proxy ping request through to backend

## Development

```bash
# Run tests
cargo test --workspace

# Run clippy
cargo clippy --workspace --all-targets

# Format code
cargo fmt --all
```

## License

MIT
