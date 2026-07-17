# Configuration

Spring Proxy uses **TOML** configuration. The top-level structure is:

```toml
[log]
level = "info"

[[services]]
# ...

[router]
# ...

[[outbounds]]
# ...

[lists]
# ...
```

## `log`

| Field | Type   | Default | Description               |
|-------|--------|---------|---------------------------|
| level | string | `info`  | Log level (error/warn/info/debug/trace) |

## `services`

Each `[[services]]` block defines a listening endpoint.

| Field                  | Type    | Default | Description                                      |
|------------------------|---------|---------|--------------------------------------------------|
| name                   | string  | —       | Unique service name                              |
| target_address         | string  | `""`    | Legacy: direct target address                    |
| target_port            | u16     | `0`     | Legacy: direct target port                       |
| listen                 | u16     | `0`     | Port to listen on (binds `0.0.0.0`)              |
| enable_proxy_protocol  | bool    | `false` | Enable HAProxy PROXY protocol on inbound         |
| ip_access.mode         | string  | `""`    | IP access mode: `allow`, `block`, or empty       |
| ip_access.list_tags    | array   | `[]`    | Tags referencing `[lists]` entries               |
| ip_access.lower_case   | bool    | `false` | Convert IPs to lowercase before matching         |
| minecraft              | table   | —       | Minecraft-specific options (see below)           |
| outbound               | string  | `""`    | Outbound name for legacy direct mode             |

### `minecraft` service options

| Field                      | Type   | Default | Description                                      |
|----------------------------|--------|---------|--------------------------------------------------|
| enable_hostname_rewrite    | bool   | `false` | Rewrite the hostname sent to backend             |
| rewritten_hostname         | string | `""`    | Hostname to send if rewrite enabled              |
| online_count.max           | i32    | `0`     | Max players shown in MOTD                        |
| online_count.online        | i32    | `0`     | Online players shown (-1 = live count)           |
| online_count.enable_max_limit | bool | `false` | Enforce max player limit                         |
| ignore_fml_suffix          | bool   | `false` | Strip Forge Mod Loader suffix from hostname      |
| ignore_srv_redirect        | bool   | `false` | Ignore SRV redirects                             |
| hostname_access.mode       | string | `""`    | Hostname access mode                             |
| name_access.mode           | string | `""`    | Player name access mode                          |
| ping_mode                  | string | `""`    | Ping mode: `disconnect`, `0ms`, or default       |
| motd_favicon              | string | `""`    | Base64 favicon or `{DEFAULT_MOTD}`               |
| motd_description          | string | `""`    | MOTD description with placeholders               |

### MOTD placeholders

| Placeholder    | Replacement                                |
|----------------|--------------------------------------------|
| `{INFO}`       | `Spring Proxy {version}`                   |
| `{NAME}`       | Outbound name                              |
| `{HOST}`       | Target address                             |
| `{PORT}`       | Target port                                |
| `{DEFAULT_MOTD}` | Built-in default favicon                 |

## `router`

| Field            | Type   | Default  | Description                                      |
|------------------|--------|----------|--------------------------------------------------|
| default_outbound | string | `""`     | Fallback outbound when no rule matches           |
| rules            | array  | `[]`     | Ordered list of routing rules                    |

### `router.rules`

| Field      | Type           | Default | Description                                      |
|------------|----------------|---------|--------------------------------------------------|
| type       | string         | —       | Rule type (see [Rules](./rules.md))              |
| parameter  | any            | —       | Rule-specific parameter                          |
| rewrite    | table          | —       | Rewrite configuration                            |
| sniff      | array          | `[]`    | Protocols to sniff (`["minecraft"]`)             |
| outbound   | string         | `""`    | Outbound to use when rule matches                |
| invert     | bool           | `false` | Invert the match result                          |

### `rewrite`

| Field                  | Type   | Default | Description                                      |
|------------------------|--------|---------|--------------------------------------------------|
| target_address         | string | `""`    | Override target address                          |
| target_port            | u16    | `0`     | Override target port                             |
| minecraft.hostname     | string | `""`    | Rewrite Minecraft hostname                       |
| minecraft.port         | u16    | `0`     | Rewrite Minecraft port                           |
| minecraft.intent       | i8     | `0`     | Rewrite next state (1=status, 2=login, 3=transfer) |

## `outbounds`

Each `[[outbounds]]` block defines a destination for routed connections.

| Field                    | Type   | Default | Description                                      |
|--------------------------|--------|---------|--------------------------------------------------|
| name                     | string | —       | Unique outbound name                             |
| dialer                   | string | `""`    | Reference to another outbound dialer             |
| target_address           | string | `""`    | TCP target address                               |
| target_port              | u16    | `0`     | TCP target port                                  |
| minecraft                | table  | —       | Minecraft-specific outbound options              |
| proxy_protocol_version   | i8     | `0`     | PROXY protocol version (0=off, 1=v1, 2=v2)       |
| proxy_options            | table  | —       | SOCKS proxy options                              |

### `proxy_options`

| Field    | Type   | Default | Description                                      |
|----------|--------|---------|--------------------------------------------------|
| type     | string | `""`    | Proxy type (e.g., `socks5`)                      |
| network  | string | `""`    | Network type                                     |
| address  | string | `""`    | Proxy server address                             |

## `lists`

Named string sets for access control. Referenced by `list_tags` in access configs.

```toml
[lists.whitelist]
mode = "allow"
entries = ["player1", "player2"]

[lists.blacklist]
mode = "block"
entries = ["griefer"]
```

Each list is a `StringSet` that can be used by multiple access controls.
