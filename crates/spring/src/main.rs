//! Spring Proxy — high-performance TCP relay for Minecraft.
//!
//! This binary is a thin entry point. All logic lives in the `spring` library
//! behind deep module interfaces.

use std::path::PathBuf;

use spring::Runner;
use spring::config::Root;

fn main() {
    // Parse CLI args manually (keeping deps minimal)
    let args: Vec<String> = std::env::args().collect();
    let config_path = args
        .iter()
        .position(|a| a == "--config" || a == "-c")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("spring.toml"));

    // Init logging
    flexi_logger::Logger::try_with_env_or_str("info")
        .expect("failed to initialise logger")
        .start()
        .expect("failed to start logger");

    log::info!("Spring Proxy v{}", env!("CARGO_PKG_VERSION"));

    // Load config
    let config: Root = match std::fs::read_to_string(&config_path) {
        Ok(content) => toml::from_str(&content).expect("failed to parse config"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::warn!(
                "Config file {:?} not found, generating default",
                config_path
            );
            let default = Root::generate_default();
            let toml_str = toml::to_string_pretty(&default).expect("failed to serialize config");
            match std::fs::write(&config_path, &toml_str) {
                Ok(_) => log::info!("Created default config at {:?}", config_path),
                Err(e) => log::warn!("Could not create default config: {e}"),
            }
            default
        }
        Err(e) => {
            panic!("failed to read config {:?}: {e}", config_path);
        }
    };

    log::info!("Config loaded: {:?}", config_path);

    // Run the proxy
    let runner = Runner::new(config);

    smol::block_on(async {
        if let Err(e) = runner.run().await {
            log::error!("Proxy exited with error: {e}");
        }
    });
}
