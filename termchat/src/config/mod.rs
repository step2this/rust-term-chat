//! Configuration system for the `TermChat` client.
//!
//! Supports layered configuration with the following priority (highest first):
//! 1. CLI arguments
//! 2. Environment variables (via clap `env` attribute)
//! 3. TOML config file (`~/.config/termchat/config.toml`)
//! 4. Compiled defaults
//!
//! Missing config file is not an error (defaults are used). An explicit
//! `--config` path that doesn't exist is an error.

use std::path::PathBuf;
use std::time::Duration;

use crate::net::NetConfig;

/// Errors that can occur when loading configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read the configuration file.
    #[error("failed to read config file {path}: {source}")]
    ReadFile {
        /// Path that was attempted.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },

    /// Failed to parse the TOML configuration.
    #[error("failed to parse config file: {0}")]
    ParseToml(#[from] toml::de::Error),

    /// Could not determine the user's config directory.
    #[error("could not determine config directory (no HOME or XDG_CONFIG_HOME)")]
    NoConfigDir,
}

// ---------------------------------------------------------------------------
// TOML file structs (all fields Option for partial overrides)
// ---------------------------------------------------------------------------

/// Top-level TOML config file structure.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct ConfigFile {
    network: NetworkFileConfig,
    chat: ChatFileConfig,
    ui: UiFileConfig,
    agent: AgentFileConfig,
}

/// `[network]` section of the config file.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct NetworkFileConfig {
    relay_url: Option<String>,
    peer_id: Option<String>,
    remote_peer: Option<String>,
    connect_timeout_secs: Option<u64>,
    register_timeout_secs: Option<u64>,
    channel_capacity: Option<usize>,
}

/// `[chat]` section of the config file.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct ChatFileConfig {
    send_retries: Option<u32>,
    ack_timeout_secs: Option<u64>,
    ack_retries: Option<u32>,
    max_payload_size: Option<usize>,
    max_duplicate_tracking: Option<usize>,
    clock_skew_tolerance_secs: Option<u64>,
    chat_event_buffer: Option<usize>,
}

/// `[ui]` section of the config file.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct UiFileConfig {
    poll_timeout_ms: Option<u64>,
    typing_timeout_secs: Option<u64>,
    timestamp_format: Option<String>,
    max_task_title_len: Option<usize>,
}

/// `[agent]` section of the config file.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct AgentFileConfig {
    socket_dir: Option<String>,
}

// ---------------------------------------------------------------------------
// Resolved configuration (concrete types, all fields populated)
// ---------------------------------------------------------------------------

/// Chat subsystem configuration (used by `ChatManager`).
#[derive(Debug, Clone)]
pub struct ChatConfig {
    /// Maximum encrypted payload size before decryption (bytes).
    pub max_payload_size: usize,
    /// Maximum number of message IDs tracked for duplicate detection.
    pub max_duplicate_tracking: usize,
    /// Clock skew tolerance in milliseconds.
    pub clock_skew_tolerance_ms: u64,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            max_payload_size: 64 * 1024,
            max_duplicate_tracking: 10_000,
            clock_skew_tolerance_ms: 5 * 60 * 1000,
        }
    }
}

/// Fully resolved client configuration.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    // -- Network --
    /// Relay server WebSocket URL.
    pub relay_url: Option<String>,
    /// Local peer identity string.
    pub peer_id: Option<String>,
    /// Remote peer identity string.
    pub remote_peer: Option<String>,
    /// Timeout for connecting to the relay server.
    pub connect_timeout: Duration,
    /// Timeout for relay registration acknowledgment.
    pub register_timeout: Duration,
    /// Channel capacity for command/event mpsc channels.
    pub channel_capacity: usize,

    // -- Chat --
    /// Number of send retries on transport failure.
    pub send_retries: u32,
    /// Ack wait timeout.
    pub ack_timeout: Duration,
    /// Number of ack retries.
    pub ack_retries: u32,
    /// Chat subsystem config (payload size, dedup, clock skew).
    pub chat: ChatConfig,
    /// Buffer size for the `ChatManager` event channel.
    pub chat_event_buffer: usize,

    // -- UI --
    /// Poll timeout for the TUI event loop.
    pub poll_timeout: Duration,
    /// Typing indicator timeout in seconds.
    pub typing_timeout_secs: u64,
    /// Timestamp display format string (chrono).
    pub timestamp_format: String,
    /// Maximum task title length in characters.
    pub max_task_title_len: usize,

    // -- Agent --
    /// Directory for agent Unix sockets.
    pub agent_socket_dir: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            relay_url: None,
            peer_id: None,
            remote_peer: None,
            connect_timeout: Duration::from_secs(10),
            register_timeout: Duration::from_secs(5),
            channel_capacity: 256,
            send_retries: 1,
            ack_timeout: Duration::from_secs(10),
            ack_retries: 1,
            chat: ChatConfig::default(),
            chat_event_buffer: 64,
            poll_timeout: Duration::from_millis(50),
            typing_timeout_secs: 3,
            timestamp_format: "%H:%M".to_string(),
            max_task_title_len: 256,
            agent_socket_dir: "/tmp".to_string(),
        }
    }
}

impl ClientConfig {
    /// Load configuration by merging CLI args, env vars, and a TOML file.
    ///
    /// CLI args and env vars are parsed via `clap`. If `--config` is given
    /// and the file does not exist, returns an error. If no `--config` is
    /// given, the default path (`~/.config/termchat/config.toml`) is tried
    /// and silently ignored if missing.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if the explicit config file cannot be read
    /// or parsed, or if the default config directory cannot be determined
    /// when `--config` is not provided and the default path is needed.
    pub fn load(cli: &CliArgs) -> Result<Self, ConfigError> {
        let file = load_config_file(cli.config.as_deref())?;
        Ok(Self::resolve(cli, &file))
    }

    /// Resolve a `ClientConfig` from CLI args and a parsed config file.
    ///
    /// Priority: CLI > file > default. This is separated from `load()` to
    /// enable unit testing without CLI parsing.
    #[must_use]
    fn resolve(cli: &CliArgs, file: &ConfigFile) -> Self {
        let defaults = Self::default();

        Self {
            relay_url: cli
                .relay_url
                .clone()
                .or_else(|| file.network.relay_url.clone()),
            peer_id: cli.peer_id.clone().or_else(|| file.network.peer_id.clone()),
            remote_peer: cli
                .remote_peer
                .clone()
                .or_else(|| file.network.remote_peer.clone()),
            connect_timeout: file
                .network
                .connect_timeout_secs
                .map_or(defaults.connect_timeout, Duration::from_secs),
            register_timeout: file
                .network
                .register_timeout_secs
                .map_or(defaults.register_timeout, Duration::from_secs),
            channel_capacity: file
                .network
                .channel_capacity
                .unwrap_or(defaults.channel_capacity),
            send_retries: file.chat.send_retries.unwrap_or(defaults.send_retries),
            ack_timeout: file
                .chat
                .ack_timeout_secs
                .map_or(defaults.ack_timeout, Duration::from_secs),
            ack_retries: file.chat.ack_retries.unwrap_or(defaults.ack_retries),
            chat: ChatConfig {
                max_payload_size: file
                    .chat
                    .max_payload_size
                    .unwrap_or(defaults.chat.max_payload_size),
                max_duplicate_tracking: file
                    .chat
                    .max_duplicate_tracking
                    .unwrap_or(defaults.chat.max_duplicate_tracking),
                clock_skew_tolerance_ms: file
                    .chat
                    .clock_skew_tolerance_secs
                    .map_or(defaults.chat.clock_skew_tolerance_ms, |s| s * 1000),
            },
            chat_event_buffer: file
                .chat
                .chat_event_buffer
                .unwrap_or(defaults.chat_event_buffer),
            poll_timeout: file
                .ui
                .poll_timeout_ms
                .map_or(defaults.poll_timeout, Duration::from_millis),
            typing_timeout_secs: file
                .ui
                .typing_timeout_secs
                .unwrap_or(defaults.typing_timeout_secs),
            timestamp_format: file
                .ui
                .timestamp_format
                .clone()
                .unwrap_or(defaults.timestamp_format),
            max_task_title_len: file
                .ui
                .max_task_title_len
                .unwrap_or(defaults.max_task_title_len),
            agent_socket_dir: file
                .agent
                .socket_dir
                .clone()
                .unwrap_or(defaults.agent_socket_dir),
        }
    }

    /// Build a [`NetConfig`] from this configuration, if all required
    /// networking fields are present.
    ///
    /// Returns `None` if `relay_url`, `peer_id`, or `remote_peer` is missing
    /// (offline demo mode).
    #[must_use]
    pub fn to_net_config(&self) -> Option<NetConfig> {
        let relay_url = self.relay_url.clone()?;
        let local_peer_id = self.peer_id.clone()?;
        let remote_peer_id = self.remote_peer.clone()?;

        if remote_peer_id.is_empty() {
            return None;
        }

        Some(NetConfig {
            relay_url,
            local_peer_id,
            remote_peer_id,
        })
    }
}

/// CLI arguments parsed by clap.
///
/// Environment variables are supported via `env` attributes for backward
/// compatibility with the previous env-var-only configuration.
#[derive(clap::Parser, Debug, Default)]
#[command(version, about = "Terminal-native encrypted messenger")]
pub struct CliArgs {
    /// WebSocket URL of the relay server.
    #[arg(long, env = "RELAY_URL")]
    pub relay_url: Option<String>,

    /// Your local peer identity string.
    #[arg(long, env = "PEER_ID")]
    pub peer_id: Option<String>,

    /// Remote peer to chat with.
    #[arg(long, env = "REMOTE_PEER")]
    pub remote_peer: Option<String>,

    /// Path to config file (default: `~/.config/termchat/config.toml`).
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Timestamp display format (chrono format string).
    #[arg(long)]
    pub timestamp_format: Option<String>,

    /// Log level filter (trace, debug, info, warn, error).
    #[arg(long, default_value = "info", env = "TERMCHAT_LOG")]
    pub log_level: String,

    /// Path to log file (default: `$TMPDIR/termchat.log`).
    #[arg(long)]
    pub log_file: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Load and parse a TOML config file.
///
/// If `explicit_path` is `Some`, the file must exist (error if not).
/// If `explicit_path` is `None`, the default path is tried and missing file
/// is treated as empty config.
fn load_config_file(explicit_path: Option<&std::path::Path>) -> Result<ConfigFile, ConfigError> {
    let path = if let Some(p) = explicit_path {
        let contents = std::fs::read_to_string(p).map_err(|e| ConfigError::ReadFile {
            path: p.to_path_buf(),
            source: e,
        })?;
        return Ok(toml::from_str(&contents)?);
    } else {
        let Some(config_dir) = dirs::config_dir() else {
            // No config dir available — use defaults.
            return Ok(ConfigFile::default());
        };
        config_dir.join("termchat").join("config.toml")
    };

    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(toml::from_str(&contents)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigFile::default()),
        Err(e) => Err(ConfigError::ReadFile { path, source: e }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_current_hardcoded_values() {
        let config = ClientConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.register_timeout, Duration::from_secs(5));
        assert_eq!(config.channel_capacity, 256);
        assert_eq!(config.send_retries, 1);
        assert_eq!(config.ack_timeout, Duration::from_secs(10));
        assert_eq!(config.ack_retries, 1);
        assert_eq!(config.chat.max_payload_size, 64 * 1024);
        assert_eq!(config.chat.max_duplicate_tracking, 10_000);
        assert_eq!(config.chat.clock_skew_tolerance_ms, 5 * 60 * 1000);
        assert_eq!(config.chat_event_buffer, 64);
        assert_eq!(config.poll_timeout, Duration::from_millis(50));
        assert_eq!(config.typing_timeout_secs, 3);
        assert_eq!(config.timestamp_format, "%H:%M");
        assert_eq!(config.max_task_title_len, 256);
        assert_eq!(config.agent_socket_dir, "/tmp");
    }

    #[test]
    fn chat_config_defaults() {
        let cc = ChatConfig::default();
        assert_eq!(cc.max_payload_size, 64 * 1024);
        assert_eq!(cc.max_duplicate_tracking, 10_000);
        assert_eq!(cc.clock_skew_tolerance_ms, 300_000);
    }

    #[test]
    fn toml_parsing_full() {
        let toml_str = r#"
[network]
relay_url = "ws://example.com:9000/ws"
peer_id = "alice"
remote_peer = "bob"
connect_timeout_secs = 30
register_timeout_secs = 10
channel_capacity = 512

[chat]
send_retries = 3
ack_timeout_secs = 20
ack_retries = 2
max_payload_size = 32768
max_duplicate_tracking = 5000
clock_skew_tolerance_secs = 600
chat_event_buffer = 128

[ui]
poll_timeout_ms = 100
typing_timeout_secs = 5
timestamp_format = "%H:%M:%S"
max_task_title_len = 512

[agent]
socket_dir = "/var/run"
"#;
        let file: ConfigFile = toml::from_str(toml_str).unwrap();
        let cli = CliArgs::default();
        let config = ClientConfig::resolve(&cli, &file);

        assert_eq!(
            config.relay_url.as_deref(),
            Some("ws://example.com:9000/ws")
        );
        assert_eq!(config.peer_id.as_deref(), Some("alice"));
        assert_eq!(config.remote_peer.as_deref(), Some("bob"));
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert_eq!(config.register_timeout, Duration::from_secs(10));
        assert_eq!(config.channel_capacity, 512);
        assert_eq!(config.send_retries, 3);
        assert_eq!(config.ack_timeout, Duration::from_secs(20));
        assert_eq!(config.ack_retries, 2);
        assert_eq!(config.chat.max_payload_size, 32768);
        assert_eq!(config.chat.max_duplicate_tracking, 5000);
        assert_eq!(config.chat.clock_skew_tolerance_ms, 600_000);
        assert_eq!(config.chat_event_buffer, 128);
        assert_eq!(config.poll_timeout, Duration::from_millis(100));
        assert_eq!(config.typing_timeout_secs, 5);
        assert_eq!(config.timestamp_format, "%H:%M:%S");
        assert_eq!(config.max_task_title_len, 512);
        assert_eq!(config.agent_socket_dir, "/var/run");
    }

    #[test]
    fn toml_parsing_partial() {
        let toml_str = r#"
[network]
relay_url = "ws://custom:9000/ws"
"#;
        let file: ConfigFile = toml::from_str(toml_str).unwrap();
        let cli = CliArgs::default();
        let config = ClientConfig::resolve(&cli, &file);

        assert_eq!(config.relay_url.as_deref(), Some("ws://custom:9000/ws"));
        // Everything else should be default.
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.channel_capacity, 256);
        assert_eq!(config.typing_timeout_secs, 3);
    }

    #[test]
    fn toml_parsing_empty() {
        let file: ConfigFile = toml::from_str("").unwrap();
        let cli = CliArgs::default();
        let config = ClientConfig::resolve(&cli, &file);

        assert!(config.relay_url.is_none());
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
    }

    #[test]
    fn cli_overrides_file() {
        let toml_str = r#"
[network]
relay_url = "ws://file:9000/ws"
peer_id = "file-peer"
"#;
        let file: ConfigFile = toml::from_str(toml_str).unwrap();
        let cli = CliArgs {
            relay_url: Some("ws://cli:9000/ws".to_string()),
            peer_id: None, // not set on CLI — should fall through to file
            ..Default::default()
        };
        let config = ClientConfig::resolve(&cli, &file);

        assert_eq!(config.relay_url.as_deref(), Some("ws://cli:9000/ws"));
        assert_eq!(config.peer_id.as_deref(), Some("file-peer"));
    }

    #[test]
    fn missing_config_file_returns_defaults() {
        let result = load_config_file(None);
        assert!(result.is_ok());
    }

    #[test]
    fn explicit_missing_config_file_returns_error() {
        let result = load_config_file(Some(std::path::Path::new("/nonexistent/config.toml")));
        assert!(result.is_err());
        assert!(matches!(result, Err(ConfigError::ReadFile { .. })));
    }

    #[test]
    fn to_net_config_returns_some_when_complete() {
        let config = ClientConfig {
            relay_url: Some("ws://localhost:9000/ws".to_string()),
            peer_id: Some("alice".to_string()),
            remote_peer: Some("bob".to_string()),
            ..Default::default()
        };
        let net = config.to_net_config();
        assert!(net.is_some());
        let net = net.unwrap();
        assert_eq!(net.relay_url, "ws://localhost:9000/ws");
        assert_eq!(net.local_peer_id, "alice");
        assert_eq!(net.remote_peer_id, "bob");
    }

    #[test]
    fn to_net_config_returns_none_when_incomplete() {
        let config = ClientConfig {
            relay_url: Some("ws://localhost:9000/ws".to_string()),
            peer_id: None,
            ..Default::default()
        };
        assert!(config.to_net_config().is_none());
    }

    #[test]
    fn to_net_config_returns_none_when_remote_peer_empty() {
        let config = ClientConfig {
            relay_url: Some("ws://localhost:9000/ws".to_string()),
            peer_id: Some("alice".to_string()),
            remote_peer: Some(String::new()),
            ..Default::default()
        };
        assert!(config.to_net_config().is_none());
    }
}
