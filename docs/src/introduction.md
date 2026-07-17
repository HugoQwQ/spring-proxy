# Spring Proxy

Spring Proxy is a high-performance TCP relay for Minecraft, inspired by [ZBProxy](https://github.com/layou233/ZBProxy). It acts as a transparent proxy between Minecraft clients and backend servers, with support for handshake sniffing, rule-based routing, custom MOTD responses, access control, and more.

## What it does

- **Sniffs Minecraft handshakes** to extract protocol version, target hostname, player name, and UUID
- **Routes connections** based on configurable rules (source IP, Minecraft hostname, player name, etc.)
- **Relays TCP traffic** bidirectionally between clients and backends
- **Provides custom MOTD** responses without hitting the backend
- **Enforces access control** by player name or hostname
- **Limits concurrent players** per outbound

## When to use it

- You need to proxy multiple Minecraft servers through a single entry point
- You want to add access control or player limits in front of an existing server
- You need to rewrite hostnames or strip FML suffixes
- You want custom MOTD responses for different entry points

## Design philosophy

Spring Proxy follows a **deep module** design:

- Each crate exposes a small, well-defined interface
- Complex logic (packet parsing, relay, routing) is hidden behind simple APIs
- The `protocol` crate knows everything about Minecraft packets; `spring` only calls `sniff_full()` and `relay()`
