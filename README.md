# Spring Proxy

A high-performance TCP relay for Minecraft based on Rust, inspired by [ZBProxy](https://github.com/layou233/ZBProxy).

## Features

- **Minecraft handshake sniffing** — inspects the first packet to extract protocol version, target hostname, player name, and UUID
- **Rule-based routing** — route connections based on service name, source IP/port, Minecraft hostname, player name, and more
- **Minecraft outbound proxy** — full Minecraft proxy
- **Plain TCP relay** — transparent bidirectional relay for any TCP traffic

## License

MIT
