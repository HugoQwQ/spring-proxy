# Development

## Running tests

```bash
# All workspace tests
cargo test --workspace

# With output
cargo test --workspace -- --nocapture

# Specific crate
cargo test -p protocol
cargo test -p spring
```

## Linting and formatting

```bash
cargo clippy --workspace --all-targets
cargo fmt --all
```

## Adding a new rule type

1. **Add the rule struct** in `crates/spring/src/route/rules.rs`:

```rust
pub struct RuleMyRule {
    config: RuleConfig,
    // rule-specific state
}

impl RuleMyRule {
    pub fn new(config: RuleConfig) -> Result<Self, common::Error> {
        // parse parameter
        Ok(Self { config })
    }
}

impl Rule for RuleMyRule {
    fn matches(&self, metadata: &ConnectionMetadata) -> bool {
        // evaluation logic
        true
    }

    fn config(&self) -> &RuleConfig {
        &self.config
    }
}
```

2. **Register it** in `crates/spring/src/route/mod.rs`:

```rust
"MyRule" => Ok(Box::new(rules::RuleMyRule::new(config)?)),
```

3. **Add tests** in `crates/spring/src/route/rules.rs`.

## Adding a new config field

1. Add the field to the appropriate struct in `crates/spring/src/config.rs` with `#[serde(default)]`
2. Use it in the relevant module (outbound, service, etc.)
3. Update the default config generation in `Root::generate_default()` if it should appear in the default config
4. Update `docs/src/configuration.md`

## Logging conventions

- **`INFO`** — Lifecycle events (service start, connection created/closed)
- **`WARN`** — Rejections, limits hit, config issues, connection errors
- **`DEBUG`** — Per-packet details, sniffing internals
- **`TRACE`** — Rule evaluation steps

## Debugging connection issues

Set `RUST_LOG=debug` to see:

- Sniffing results (protocol version, player name, hostname)
- Rule match outcomes
- Relay start/stop events

Set `RUST_LOG=spring::outbound::minecraft=debug` for just Minecraft outbound details.
