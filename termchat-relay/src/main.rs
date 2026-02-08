//! `TermChat` Relay Server -- lightweight store-and-forward relay.
//!
//! An axum WebSocket server that routes encrypted payloads between
//! `TermChat` peers. The relay never sees plaintext -- it only forwards
//! opaque encrypted blobs identified by `PeerId`.
//!
//! # Usage
//!
//! ```bash
//! # Run on default address 0.0.0.0:9000
//! cargo run --bin termchat-relay
//!
//! # Run on custom address
//! cargo run --bin termchat-relay -- --bind 127.0.0.1:8080
//!
//! # Or via environment variable (backward compatible)
//! RELAY_ADDR=127.0.0.1:8080 cargo run --bin termchat-relay
//! ```

use std::sync::Arc;

use clap::Parser;
use termchat_relay::config::{RelayCliArgs, RelayConfig};
use termchat_relay::relay::{self, RelayState};
use termchat_relay::store::MessageStore;

#[tokio::main]
async fn main() {
    let cli = RelayCliArgs::parse();

    // Load config from CLI args + config file + env vars + defaults.
    let config = match RelayConfig::load(&cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading configuration: {e}");
            std::process::exit(1);
        }
    };

    // Initialize tracing with the resolved log level.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    tracing::info!(addr = %config.bind_addr, "starting termchat relay server");

    let store = MessageStore::with_max_queue_size(config.max_queue_size);
    let state = Arc::new(RelayState::with_config(config.max_payload_size, store));

    match relay::start_server_with_state(&config.bind_addr, state).await {
        Ok((bound_addr, handle)) => {
            tracing::info!(addr = %bound_addr, "relay server listening");
            if let Err(e) = handle.await {
                tracing::error!(error = %e, "relay server task failed");
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to start relay server");
            std::process::exit(1);
        }
    }
}
