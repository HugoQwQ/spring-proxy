# Spring Proxy
[![FOSSA Status](https://app.fossa.com/api/projects/git%2Bgithub.com%2FHugoQwQ%2Fspring-proxy.svg?type=shield)](https://app.fossa.com/projects/git%2Bgithub.com%2FHugoQwQ%2Fspring-proxy?ref=badge_shield)


A high-performance TCP relay for Minecraft based on Rust, inspired by [ZBProxy](https://github.com/layou233/ZBProxy).

## Features

- **Minecraft handshake sniffing** — inspects the first packet to extract protocol version, target hostname, player name, and UUID
- **Rule-based routing** — route connections based on service name, source IP/port, Minecraft hostname, player name, and more
- **Minecraft outbound proxy** — full Minecraft proxy
- **Plain TCP relay** — transparent bidirectional relay for any TCP traffic

## License

MIT


[![FOSSA Status](https://app.fossa.com/api/projects/git%2Bgithub.com%2FHugoQwQ%2Fspring-proxy.svg?type=large)](https://app.fossa.com/projects/git%2Bgithub.com%2FHugoQwQ%2Fspring-proxy?ref=badge_large)