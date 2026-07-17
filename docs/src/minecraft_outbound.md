# Minecraft Outbound

A `MinecraftOutbound` handles the server-side of Minecraft connections. It can respond to status (MOTD) requests itself, or proxy them from the backend. For login connections, it performs access control and then relays to the backend.

## Enabling

Add a `minecraft` table to an outbound:

```toml
[[outbounds]]
name = "Hypixel-out"
target_address = "mc.hypixel.net"
target_port = 25565

[outbounds.minecraft]
motd_favicon = "{DEFAULT_MOTD}"
motd_description = "§dSpring Proxy§r"
online_count.max = 20
online_count.online = -1
```

If the `minecraft` table is absent, the outbound acts as a **plain TCP relay**.

## MOTD

### Custom MOTD

When `motd_favicon` or `motd_description` is non-empty, the proxy generates its own MOTD response:

```toml
[outbounds.minecraft]
motd_favicon = "{DEFAULT_MOTD}"
motd_description = "Welcome to §a{HOST}§r!"
```

### Proxy MOTD from backend

When both `motd_favicon` and `motd_description` are empty, the proxy forwards the status request to the backend and relays the response back:

```toml
[outbounds.minecraft]
# Empty MOTD fields = proxy from backend
motd_favicon = ""
motd_description = ""
```

### MOTD placeholders

| Placeholder      | Replacement                                  |
|------------------|----------------------------------------------|
| `{INFO}`         | `Spring Proxy {version}`                     |
| `{NAME}`         | Outbound name                                |
| `{HOST}`         | Target address                               |
| `{PORT}`         | Target port                                  |
| `{DEFAULT_MOTD}` | Built-in default favicon (64×64 PNG, Base64) |

## Ping modes

Control how ping (latency measurement) is handled:

| Mode           | Behaviour                                              |
|----------------|--------------------------------------------------------|
| `"disconnect"` | Send MOTD, then close the connection immediately       |
| `"0ms"`        | Respond to ping with a 0ms latency timestamp           |
| `""` (default) | Read the ping request and echo it back (proxy through) |

```toml
[outbounds.minecraft]
ping_mode = "0ms"
```

## Access control

### Hostname access

Block or allow connections based on the hostname in the handshake:

```toml
[outbounds.minecraft.hostname_access]
mode = "block"
list_tags = ["bad-hosts"]
```

### Name access

Block or allow connections based on the player name:

```toml
[outbounds.minecraft.name_access]
mode = "allow"
list_tags = ["whitelist"]
lower_case = true
```

When `lower_case = true`, player names are converted to lowercase before matching.

## Player limits

Enforce a maximum number of concurrent players:

```toml
[outbounds.minecraft.online_count]
max = 100
online = -1
enable_max_limit = true
```

| Field              | Description                                                 |
|--------------------|-------------------------------------------------------------|
| `max`              | Maximum concurrent players                                  |
| `online`           | Current online count in MOTD (`-1` = live count from proxy) |
| `enable_max_limit` | Actually enforce the limit (kick players when full)         |

When the limit is reached, new login attempts receive a kick message:

> The player number limiter of `{outbound}` has been exceeded. You have been disconnected.

## Hostname rewrite

Change the hostname sent to the backend server:

```toml
[outbounds.minecraft]
enable_hostname_rewrite = true
rewritten_hostname = "backend.internal"
```

If `rewritten_hostname` is empty, the outbound's `target_address` is used instead.

## FML handling

Forge Mod Loader sends extra data after the hostname (`\x00FML\x01` etc.). By default, this suffix is preserved and sent to the backend.

To strip it:

```toml
[outbounds.minecraft]
ignore_fml_suffix = true
```

To append the FML markup to a rewritten hostname, leave `ignore_fml_suffix = false` (default).
