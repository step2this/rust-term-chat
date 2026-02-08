//! Configuration system for the `TermChat` relay server.
//!
//! Supports layered configuration with the following priority (highest first):
//! 1. CLI arguments
//! 2. Environment variables (via clap `env` attribute)
//! 3. TOML config file (`~/.config/termchat-relay/config.toml`)
//! 4. Compiled defaults

use std::path::PathBuf;

/// Errors that can occur when loading relay configuration.
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
}

// ---------------------------------------------------------------------------
// TOML file structs (all fields Option for partial overrides)
// ---------------------------------------------------------------------------

/// Top-level TOML config file structure for the relay.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct RelayConfigFile {
    server: ServerFileConfig,
}

/// `[server]` section of the relay config file.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct ServerFileConfig {
    bind_addr: Option<String>,
    max_payload_size: Option<usize>,
    max_queue_size: Option<usize>,
}

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

/// CLI arguments for the relay server.
#[derive(clap::Parser, Debug, Default)]
#[command(version, about = "TermChat relay server")]
pub struct RelayCliArgs {
    /// Address to bind the relay server to.
    #[arg(short, long, env = "RELAY_ADDR")]
    pub bind: Option<String>,

    /// Path to config file (default: `~/.config/termchat-relay/config.toml`).
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Maximum payload size in bytes.
    #[arg(long)]
    pub max_payload_size: Option<usize>,

    /// Maximum queue size per offline peer.
    #[arg(long)]
    pub max_queue_size: Option<usize>,

    /// Log level filter (trace, debug, info, warn, error).
    #[arg(long, default_value = "info", env = "RELAY_LOG")]
    pub log_level: String,
}

// ---------------------------------------------------------------------------
// Resolved configuration
// ---------------------------------------------------------------------------

/// Fully resolved relay server configuration.
#[derive(Debug, Clone)]
pub struct RelayConfig {
    /// Address to bind the server to (e.g., `0.0.0.0:9000`).
    pub bind_addr: String,
    /// Maximum allowed payload size in bytes.
    pub max_payload_size: usize,
    /// Maximum number of queued messages per offline peer.
    pub max_queue_size: usize,
    /// Log level filter string.
    pub log_level: String,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:9000".to_string(),
            max_payload_size: 64 * 1024,
            max_queue_size: 1000,
            log_level: "info".to_string(),
        }
    }
}

impl RelayConfig {
    /// Load configuration by merging CLI args, env vars, and a TOML file.
    ///
    /// If `--config` is given and the file does not exist, returns an error.
    /// If no `--config` is given, the default path is tried and missing file
    /// is treated as empty config.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if the explicit config file cannot be read
    /// or parsed.
    pub fn load(cli: &RelayCliArgs) -> Result<Self, ConfigError> {
        let file = load_config_file(cli.config.as_deref())?;
        Ok(Self::resolve(cli, &file))
    }

    /// Resolve a `RelayConfig` from CLI args and a parsed config file.
    ///
    /// Priority: CLI > file > default.
    #[must_use]
    fn resolve(cli: &RelayCliArgs, file: &RelayConfigFile) -> Self {
        let defaults = Self::default();

        Self {
            bind_addr: cli
                .bind
                .clone()
                .or_else(|| file.server.bind_addr.clone())
                .unwrap_or(defaults.bind_addr),
            max_payload_size: cli
                .max_payload_size
                .or(file.server.max_payload_size)
                .unwrap_or(defaults.max_payload_size),
            max_queue_size: cli
                .max_queue_size
                .or(file.server.max_queue_size)
                .unwrap_or(defaults.max_queue_size),
            log_level: cli.log_level.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Load and parse a TOML config file for the relay.
fn load_config_file(
    explicit_path: Option<&std::path::Path>,
) -> Result<RelayConfigFile, ConfigError> {
    let path = if let Some(p) = explicit_path {
        let contents = std::fs::read_to_string(p).map_err(|e| ConfigError::ReadFile {
            path: p.to_path_buf(),
            source: e,
        })?;
        return Ok(toml::from_str(&contents)?);
    } else {
        let Some(config_dir) = dirs::config_dir() else {
            return Ok(RelayConfigFile::default());
        };
        config_dir.join("termchat-relay").join("config.toml")
    };

    match std::fs::read_to_string(&path) {
        Ok(contents) => Ok(toml::from_str(&contents)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RelayConfigFile::default()),
        Err(e) => Err(ConfigError::ReadFile { path, source: e }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_current_hardcoded_values() {
        let config = RelayConfig::default();
        assert_eq!(config.bind_addr, "0.0.0.0:9000");
        assert_eq!(config.max_payload_size, 64 * 1024);
        assert_eq!(config.max_queue_size, 1000);
    }

    #[test]
    fn toml_parsing_full() {
        let toml_str = r#"
[server]
bind_addr = "127.0.0.1:8080"
max_payload_size = 32768
max_queue_size = 500
"#;
        let file: RelayConfigFile = toml::from_str(toml_str).unwrap();
        let cli = RelayCliArgs::default();
        let config = RelayConfig::resolve(&cli, &file);

        assert_eq!(config.bind_addr, "127.0.0.1:8080");
        assert_eq!(config.max_payload_size, 32768);
        assert_eq!(config.max_queue_size, 500);
    }

    #[test]
    fn toml_parsing_partial() {
        let toml_str = r#"
[server]
max_queue_size = 2000
"#;
        let file: RelayConfigFile = toml::from_str(toml_str).unwrap();
        let cli = RelayCliArgs::default();
        let config = RelayConfig::resolve(&cli, &file);

        assert_eq!(config.bind_addr, "0.0.0.0:9000"); // default
        assert_eq!(config.max_payload_size, 64 * 1024); // default
        assert_eq!(config.max_queue_size, 2000); // from file
    }

    #[test]
    fn toml_parsing_empty() {
        let file: RelayConfigFile = toml::from_str("").unwrap();
        let cli = RelayCliArgs::default();
        let config = RelayConfig::resolve(&cli, &file);

        assert_eq!(config.bind_addr, "0.0.0.0:9000");
        assert_eq!(config.max_payload_size, 64 * 1024);
        assert_eq!(config.max_queue_size, 1000);
    }

    #[test]
    fn cli_overrides_file() {
        let toml_str = r#"
[server]
bind_addr = "127.0.0.1:8080"
max_payload_size = 32768
"#;
        let file: RelayConfigFile = toml::from_str(toml_str).unwrap();
        let cli = RelayCliArgs {
            bind: Some("0.0.0.0:3000".to_string()),
            max_payload_size: None, // not set on CLI — should fall through to file
            ..Default::default()
        };
        let config = RelayConfig::resolve(&cli, &file);

        assert_eq!(config.bind_addr, "0.0.0.0:3000"); // from CLI
        assert_eq!(config.max_payload_size, 32768); // from file
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
}
