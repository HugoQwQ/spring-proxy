# Quick Start

## Installation

```bash
cargo build --release
```

The binary is at `./target/release/spring`.

## First run

Spring Proxy looks for `spring.toml` in the working directory. If not found, it generates a default config:

```bash
./target/release/spring
```

Output:
```
INFO [spring] Spring Proxy v0.1.0
WARN [spring] Config file "spring.toml" not found, generating default
INFO [spring] Created default config at "spring.toml"
INFO [spring::runner] Spring Proxy v0.1.0
INFO [spring::runner] Initialized 1 outbounds
INFO [spring::runner] Initialized router with 2 rules
INFO [spring::runner] Started 1 services. Press Ctrl+C to stop.
INFO [spring::service] Service 'Hypixel-in' listening on 0.0.0.0:25565
```

## Custom config path

```bash
./target/release/spring --config /etc/spring/proxy.toml
```

## Default config explained

The generated default config creates a single service listening on `0.0.0.0:25565` that routes Minecraft connections to `mc.hypixel.net:25565` with a custom MOTD.

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

## Logging levels

Set via `log.level` or the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug ./target/release/spring
```

Levels: `error`, `warn`, `info`, `debug`, `trace`
