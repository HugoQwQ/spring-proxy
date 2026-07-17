# Routing Rules

The router evaluates rules in order. The **first matching rule wins** and its outbound is used. If no rule matches, the `default_outbound` is used.

## Rule structure

```toml
[[router.rules]]
type = "MinecraftHostname"
parameter = "hypixel.net"
outbound = "Hypixel-out"
invert = false
```

## Rule types

### `always`

Always matches. Useful as a catch-all or to enable sniffing for all connections.

```toml
[[router.rules]]
type = "always"
sniff = ["minecraft"]
```

### `and`

All sub-rules must match.

```toml
[[router.rules]]
type = "and"
parameter = [
    { type = "MinecraftStatus" },
    { type = "MinecraftHostname", parameter = "hypixel.net" }
]
outbound = "status-out"
```

### `or`

Any sub-rule must match.

```toml
[[router.rules]]
type = "or"
parameter = [
    { type = "ServiceName", parameter = "Lobby" },
    { type = "ServiceName", parameter = "Lobby-v6" }
]
outbound = "lobby-out"
```

### `ServiceName`

Matches the inbound service name.

```toml
[[router.rules]]
type = "ServiceName"
parameter = "Hypixel-in"
outbound = "Hypixel-out"
```

### `SourceIPVersion`

Matches by IP version (`4` or `6`).

```toml
[[router.rules]]
type = "SourceIPVersion"
parameter = 4
outbound = "ipv4-out"
```

### `SourceIP`

Matches by source IP or CIDR.

```toml
[[router.rules]]
type = "SourceIP"
parameter = "10.0.0.0/8"
outbound = "internal-out"
```

### `SourcePort`

Matches by source port (useful when NAT maps different ports to the same service).

```toml
[[router.rules]]
type = "SourcePort"
parameter = 25566
outbound = "special-out"
```

### `MinecraftHostname`

Matches by the hostname in the Minecraft handshake (supports exact and suffix matching).

```toml
[[router.rules]]
type = "MinecraftHostname"
parameter = "hypixel.net"
outbound = "hypixel-out"
```

Suffix matching is also supported: `parameter = ".hypixel.net"` matches any subdomain.

### `MinecraftPlayerName`

Matches by the player name from the Login Start packet.

```toml
[[router.rules]]
type = "MinecraftPlayerName"
parameter = "Notch"
outbound = "vip-out"
```

### `MinecraftStatus`

Matches if the handshake next state is `1` (status / server list ping).

```toml
[[router.rules]]
type = "MinecraftStatus"
outbound = "status-handler"
```

### `MinecraftTransfer`

Matches if the handshake next state is `3` (transfer, 1.20.5+).

```toml
[[router.rules]]
type = "MinecraftTransfer"
outbound = "transfer-handler"
```

## Rewrites

Rules can rewrite connection metadata before it reaches the outbound:

```toml
[[router.rules]]
type = "ServiceName"
parameter = "Hypixel-in"
outbound = "Hypixel-out"

[router.rules.rewrite.minecraft]
hostname = "mc.hypixel.net"
port = 25565
intent = 2
```

| Rewrite field | Effect                                                            |
|---------------|-------------------------------------------------------------------|
| `hostname`    | Changes the server address in the rewritten handshake             |
| `port`        | Changes the server port in the rewritten handshake                |
| `intent`      | Forces the next state (1=status, 2=login, 3=transfer)             |

## Sniffing

The `sniff` array tells the router which protocols to inspect. Currently only `"minecraft"` is supported.

```toml
[[router.rules]]
type = "always"
sniff = ["minecraft"]
```

Sniffing happens **once per connection**, upfront in the service layer. The router's `sniff` field is mostly for compatibility with ZBProxy's config format — the actual sniffing is always performed for Minecraft connections.

## Invert

Set `invert = true` to flip the match result:

```toml
[[router.rules]]
type = "MinecraftHostname"
parameter = "hypixel.net"
invert = true
outbound = "other-out"
```

This matches connections where the hostname is **NOT** `hypixel.net`.
