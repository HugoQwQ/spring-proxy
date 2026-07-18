//! Spring Proxy — high-performance TCP relay for Minecraft.
//!
//! This binary is a thin entry point. All logic lives in the `spring` library
//! behind deep module interfaces.

use std::path::PathBuf;

use spring::Runner;

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

    // Load config and run
    let runner = Runner::from_config_file(&config_path).expect("failed to load config");

    smol::block_on(async {
        if let Err(e) = runner.run().await {
            log::error!("Proxy exited with error: {e}");
        }
    });
}
