//! `TermChat` — terminal-native encrypted messenger.
//!
//! Launches the TUI and optionally connects to a relay server for live
//! messaging. Configuration via CLI flags, environment variables, or
//! config file (`~/.config/termchat/config.toml`).
//!
//! ```bash
//! # Offline demo mode
//! cargo run --bin termchat
//!
//! # Connect to a relay
//! cargo run --bin termchat -- --relay-url ws://127.0.0.1:9000/ws \
//!     --peer-id alice --remote-peer bob
//!
//! # Or via environment variables (backward compatible)
//! RELAY_URL=ws://127.0.0.1:9000/ws PEER_ID=alice REMOTE_PEER=bob cargo run
//! ```

use std::io;
use std::path::Path;

use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;
use tracing_appender::non_blocking::WorkerGuard;

use termchat::app::{App, DisplayMessage, MessageStatus, PanelFocus};
use termchat::config::{CliArgs, ClientConfig};
use termchat::net::{self, NetCommand, NetConfig, NetEvent};
use termchat::ui;

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = CliArgs::parse();

    // Load and resolve configuration (CLI args > config file > env > defaults).
    let config = match ClientConfig::load(&cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: failed to load config file: {e}");
            ClientConfig::default()
        }
    };

    // Initialize logging before terminal setup (logs go to file, not stdout).
    let _log_guard = init_logging(&cli.log_level, cli.log_file.as_deref());

    tracing::info!("termchat starting");

    // Build networking config from resolved settings.
    let net_config = config.to_net_config();

    // Set up terminal.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the app.
    let result = run_app(&mut terminal, net_config, &config).await;

    // Restore terminal.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    tracing::info!("termchat exiting");
    result
}

/// Initialize file-based logging.
///
/// Logs are written to a file (never stdout, since ratatui owns the terminal).
/// Returns a [`WorkerGuard`] that must be held until shutdown to ensure all
/// buffered log entries are flushed.
fn init_logging(level: &str, file_path: Option<&Path>) -> Option<WorkerGuard> {
    let default_path = std::env::temp_dir().join("termchat.log");
    let log_path = file_path.unwrap_or(&default_path);

    let log_dir = log_path.parent()?;
    let file_name = log_path.file_name()?.to_str()?;

    let file_appender = tracing_appender::rolling::never(log_dir, file_name);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(env_filter)
        .with_ansi(false)
        .init();

    Some(guard)
}

/// Main application loop with optional networking.
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    net_config: Option<NetConfig>,
    client_config: &ClientConfig,
) -> io::Result<()> {
    let mut app = App::new()
        .with_typing_timeout(client_config.typing_timeout_secs)
        .with_max_task_title_len(client_config.max_task_title_len);

    // Attempt to connect to the relay if config is provided.
    let (cmd_tx, mut evt_rx) = match net_config {
        Some(config) => match net::spawn_net(config).await {
            Ok((tx, rx)) => {
                app.push_system_message("Connected via Relay".to_string());
                (Some(tx), Some(rx))
            }
            Err(e) => {
                app.push_system_message(format!(
                    "Could not connect to relay — running in offline mode ({e})"
                ));
                (None, None)
            }
        },
        None => (None, None),
    };

    loop {
        // Step 1: Draw the UI frame.
        terminal.draw(|frame| ui::draw(frame, &app))?;

        // Step 2: Drain all pending NetEvents (non-blocking).
        if let Some(ref mut rx) = evt_rx {
            drain_net_events(&mut app, rx);
        }

        // Step 3: Tick typing timer.
        app.tick_typing();

        // Step 4: Poll for terminal input events.
        if event::poll(client_config.poll_timeout)?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Intercept Enter on non-slash input to send via network.
            if key.code == KeyCode::Enter
                && key.modifiers == KeyModifiers::NONE
                && app.focus == PanelFocus::Input
                && !app.input.trim().is_empty()
                && !app.input.trim().starts_with('/')
                && let Some(ref tx) = cmd_tx
            {
                let input_clone = app.input.clone();
                send_message_command(tx, &input_clone, &mut app);
            }

            app.handle_key_event(key);
        }

        if app.should_quit {
            // Send shutdown command to networking tasks.
            if let Some(ref tx) = cmd_tx {
                let _ = tx.try_send(NetCommand::Shutdown);
            }
            return Ok(());
        }
    }
}

/// Send a message command to the networking layer.
///
/// Uses `try_send` to avoid blocking. If the channel is full (back-pressure),
/// shows a system message to the user (Extension 7a).
fn send_message_command(tx: &mpsc::Sender<NetCommand>, input: &str, app: &mut App) {
    let text = input.trim().to_string();
    match tx.try_send(NetCommand::SendMessage { text }) {
        Ok(()) => {
            // Message will be sent by the background task.
            // app.handle_key_event(Enter) will add it to local display.
        }
        Err(mpsc::error::TrySendError::Full(_)) => {
            app.push_system_message("Message queued, network busy".to_string());
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            app.push_system_message("Network disconnected".to_string());
        }
    }
}

/// Drain all pending `NetEvent`s from the receiver and apply them to the app.
fn drain_net_events(app: &mut App, rx: &mut mpsc::Receiver<NetEvent>) {
    while let Ok(event) = rx.try_recv() {
        match event {
            NetEvent::MessageReceived {
                sender,
                content,
                timestamp_ms,
            } => {
                // Convert epoch ms to HH:MM display format.
                let timestamp = format_timestamp_ms(timestamp_ms);
                app.messages.push(DisplayMessage {
                    sender,
                    content,
                    timestamp,
                    status: MessageStatus::Delivered,
                });
                // Auto-scroll to bottom.
                app.message_scroll = app.messages.len().saturating_sub(1);
            }
            NetEvent::StatusChanged { delivered, .. } => {
                // Find the most recent "You" message with Sent status and update it.
                if delivered
                    && let Some(msg) = app
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|m| m.sender == "You" && m.status == MessageStatus::Sent)
                {
                    msg.status = MessageStatus::Delivered;
                }
            }
            NetEvent::ConnectionStatus {
                connected,
                transport_type,
            } => {
                if connected {
                    app.push_system_message(format!("Connected via {transport_type}"));
                } else {
                    app.push_system_message(format!("Disconnected from {transport_type}"));
                }
            }
            NetEvent::Reconnecting {
                attempt,
                max_attempts,
            } => {
                app.push_system_message(format!(
                    "Reconnecting... (attempt {attempt}/{max_attempts})"
                ));
            }
            NetEvent::ReconnectFailed => {
                app.push_system_message(
                    "Reconnection failed — will retry in background".to_string(),
                );
            }
            NetEvent::Error(msg) => {
                app.push_system_message(format!("Network error: {msg}"));
            }
        }
    }
}

/// Format an epoch-millisecond timestamp as "HH:MM".
fn format_timestamp_ms(ms: u64) -> String {
    use chrono::{Local, TimeZone};
    let secs = (ms / 1000).cast_signed();
    let nsecs = u32::try_from((ms % 1000) * 1_000_000).unwrap_or(0);
    match Local.timestamp_opt(secs, nsecs) {
        chrono::LocalResult::Single(dt) => dt.format("%H:%M").to_string(),
        _ => "??:??".to_string(),
    }
}
