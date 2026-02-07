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
//! RELAY_ADDR=127.0.0.1:8080 cargo run --bin termchat-relay
//! ```

use termchat_relay::relay;

/// Default bind address for the relay server.
const DEFAULT_ADDR: &str = "0.0.0.0:9000";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let addr = std::env::var("RELAY_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());

    tracing::info!(addr = %addr, "starting termchat relay server");

    match relay::start_server(&addr).await {
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
