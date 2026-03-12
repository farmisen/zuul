use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::ZuulError;

/// The main configuration file name.
const CONFIG_FILE: &str = ".zuul.toml";
/// The local overrides file name.
const LOCAL_CONFIG_FILE: &str = ".zuul.local.toml";

/// Raw representation of `.zuul.toml` as deserialized by serde.
#[derive(Debug, Clone, Deserialize, Default)]
struct ConfigFile {
    #[serde(default)]
    backend: BackendConfig,
    #[serde(default)]
    defaults: DefaultsConfig,
}

/// The `[backend]` section of `.zuul.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct BackendConfig {
    #[serde(rename = "type")]
    backend_type: String,
    project_id: Option<String>,
    credentials: Option<String>,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            backend_type: "gcp-secret-manager".to_string(),
            project_id: None,
            credentials: None,
        }
    }
}

/// The `[defaults]` section of `.zuul.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
struct DefaultsConfig {
    environment: Option<String>,
}

/// Raw representation of `.zuul.local.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
struct LocalConfigFile {
    #[serde(default)]
    secrets: HashMap<String, String>,
}

/// Resolved application configuration after merging all sources.
#[derive(Debug, Clone)]
pub struct Config {
    /// Backend type (e.g., "gcp-secret-manager").
    pub backend_type: String,
    /// GCP project ID.
    pub project_id: Option<String>,
    /// Path to GCP credentials file.
    pub credentials: Option<String>,
    /// Default environment name.
    pub default_environment: Option<String>,
    /// Local secret overrides from `.zuul.local.toml`.
    pub local_overrides: HashMap<String, String>,
    /// Directory where `.zuul.toml` was found.
    pub config_dir: Option<PathBuf>,
}

/// CLI-provided overrides for configuration resolution.
#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub environment: Option<String>,
    pub project_id: Option<String>,
    pub config_path: Option<PathBuf>,
}

/// Search for `.zuul.toml` starting from `start_dir` and walking up ancestors.
fn find_config_file(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir.to_path_buf();
    loop {
        let candidate = dir.join(CONFIG_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Load and resolve configuration from all sources.
///
/// Resolution order (highest priority first):
/// 1. CLI flags
/// 2. Environment variables
/// 3. `.zuul.local.toml` secret overrides
/// 4. `.zuul.toml` in current or ancestor directory
/// 5. Built-in defaults
pub fn load_config(start_dir: &Path, cli: &CliOverrides) -> Result<Config, ZuulError> {
    // Find and parse the config file
    let config_path = cli
        .config_path
        .clone()
        .or_else(|| find_config_file(start_dir));

    let (file_config, config_dir) = match &config_path {
        Some(path) => {
            let content = std::fs::read_to_string(path).map_err(|e| {
                ZuulError::Config(format!("Failed to read {}: {e}", path.display()))
            })?;
            let parsed: ConfigFile = toml::from_str(&content).map_err(|e| {
                ZuulError::Config(format!("Failed to parse {}: {e}", path.display()))
            })?;
            (parsed, path.parent().map(Path::to_path_buf))
        }
        None => (ConfigFile::default(), None),
    };

    // Parse local overrides
    let local_overrides = match &config_dir {
        Some(dir) => {
            let local_path = dir.join(LOCAL_CONFIG_FILE);
            if local_path.is_file() {
                let content = std::fs::read_to_string(&local_path).map_err(|e| {
                    ZuulError::Config(format!("Failed to read {}: {e}", local_path.display()))
                })?;
                let parsed: LocalConfigFile = toml::from_str(&content).map_err(|e| {
                    ZuulError::Config(format!("Failed to parse {}: {e}", local_path.display()))
                })?;
                parsed.secrets
            } else {
                HashMap::new()
            }
        }
        None => HashMap::new(),
    };

    // Resolve: CLI flags → env vars → config file → defaults
    let backend_type = env::var("ZUUL_BACKEND")
        .ok()
        .unwrap_or(file_config.backend.backend_type);

    let project_id = cli
        .project_id
        .clone()
        .or_else(|| env::var("ZUUL_GCP_PROJECT").ok())
        .or(file_config.backend.project_id);

    let credentials = env::var("ZUUL_GCP_CREDENTIALS")
        .ok()
        .or(file_config.backend.credentials);

    let default_environment = cli
        .environment
        .clone()
        .or_else(|| env::var("ZUUL_DEFAULT_ENV").ok())
        .or(file_config.defaults.environment);

    Ok(Config {
        backend_type,
        project_id,
        credentials,
        default_environment,
        local_overrides,
        config_dir,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serial_test::serial;

    use super::*;

    fn write_config(dir: &Path, filename: &str, content: &str) {
        fs::write(dir.join(filename), content).unwrap();
    }

    #[test]
    #[serial]
    fn parse_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        write_config(
            dir.path(),
            CONFIG_FILE,
            r#"
            [backend]
            type = "gcp-secret-manager"
            project_id = "my-project"

            [defaults]
            environment = "dev"
            "#,
        );

        let config = load_config(dir.path(), &CliOverrides::default()).unwrap();
        assert_eq!(config.backend_type, "gcp-secret-manager");
        assert_eq!(config.project_id.as_deref(), Some("my-project"));
        assert_eq!(config.default_environment.as_deref(), Some("dev"));
    }

    #[test]
    #[serial]
    fn missing_fields_use_defaults() {
        let dir = tempfile::tempdir().unwrap();
        write_config(dir.path(), CONFIG_FILE, "[backend]\n");

        let config = load_config(dir.path(), &CliOverrides::default()).unwrap();
        assert_eq!(config.backend_type, "gcp-secret-manager");
        assert_eq!(config.project_id, None);
        assert_eq!(config.default_environment, None);
    }

    #[test]
    #[serial]
    fn no_config_file_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_config(dir.path(), &CliOverrides::default()).unwrap();
        assert_eq!(config.backend_type, "gcp-secret-manager");
        assert_eq!(config.project_id, None);
    }

    #[test]
    #[serial]
    fn ancestor_directory_search() {
        let dir = tempfile::tempdir().unwrap();
        write_config(
            dir.path(),
            CONFIG_FILE,
            r#"
            [backend]
            project_id = "ancestor-project"
            "#,
        );
        let child = dir.path().join("sub").join("deep");
        fs::create_dir_all(&child).unwrap();

        let config = load_config(&child, &CliOverrides::default()).unwrap();
        assert_eq!(config.project_id.as_deref(), Some("ancestor-project"));
    }

    #[test]
    #[serial]
    fn local_overrides_parsed() {
        let dir = tempfile::tempdir().unwrap();
        write_config(dir.path(), CONFIG_FILE, "[backend]\n");
        write_config(
            dir.path(),
            LOCAL_CONFIG_FILE,
            r#"
            [secrets]
            DATABASE_URL = "postgres://localhost/mydb"
            API_KEY = "local-key"
            "#,
        );

        let config = load_config(dir.path(), &CliOverrides::default()).unwrap();
        assert_eq!(
            config
                .local_overrides
                .get("DATABASE_URL")
                .map(String::as_str),
            Some("postgres://localhost/mydb")
        );
        assert_eq!(
            config.local_overrides.get("API_KEY").map(String::as_str),
            Some("local-key")
        );
    }

    #[test]
    #[serial]
    fn env_var_overrides_config_file() {
        let dir = tempfile::tempdir().unwrap();
        write_config(
            dir.path(),
            CONFIG_FILE,
            r#"
            [backend]
            project_id = "file-project"

            [defaults]
            environment = "file-env"
            "#,
        );

        // SAFETY: test-only, these tests run serially via --test-threads=1
        // when env var tests are involved, or accept the inherent race risk in tests.
        unsafe {
            env::set_var("ZUUL_GCP_PROJECT", "env-project");
            env::set_var("ZUUL_DEFAULT_ENV", "env-env");
        }

        let config = load_config(dir.path(), &CliOverrides::default()).unwrap();
        assert_eq!(config.project_id.as_deref(), Some("env-project"));
        assert_eq!(config.default_environment.as_deref(), Some("env-env"));

        unsafe {
            env::remove_var("ZUUL_GCP_PROJECT");
            env::remove_var("ZUUL_DEFAULT_ENV");
        }
    }

    #[test]
    #[serial]
    fn cli_overrides_env_vars() {
        let dir = tempfile::tempdir().unwrap();
        write_config(dir.path(), CONFIG_FILE, "[backend]\n");

        unsafe {
            env::set_var("ZUUL_GCP_PROJECT", "env-project");
        }

        let cli = CliOverrides {
            project_id: Some("cli-project".to_string()),
            environment: Some("cli-env".to_string()),
            ..Default::default()
        };

        let config = load_config(dir.path(), &cli).unwrap();
        assert_eq!(config.project_id.as_deref(), Some("cli-project"));
        assert_eq!(config.default_environment.as_deref(), Some("cli-env"));

        unsafe {
            env::remove_var("ZUUL_GCP_PROJECT");
        }
    }

    #[test]
    #[serial]
    fn invalid_toml_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        write_config(dir.path(), CONFIG_FILE, "not valid toml [[[");

        let result = load_config(dir.path(), &CliOverrides::default());
        assert!(result.is_err());
    }
}
