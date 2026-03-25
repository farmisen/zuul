use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use console::style;

use crate::backend::Backend;
use crate::backend::gcp::GcpClient;
use crate::backend::gcp_backend::GcpBackend;
use crate::config::Config;
use crate::error::ZuulError;
use crate::prompt;

/// Run the `zuul auth` command.
///
/// Dispatches to the appropriate backend-specific auth flow.
pub async fn run(
    config: &Config,
    check: bool,
    reconfigure: bool,
    non_interactive: bool,
) -> Result<(), ZuulError> {
    match config.backend_type.as_str() {
        "gcp-secret-manager" => run_gcp(config, check, reconfigure, non_interactive).await,
        "file" => run_file(config, check),
        other => Err(ZuulError::Config(format!(
            "Unknown backend type '{other}'. Supported: gcp-secret-manager, file."
        ))),
    }
}

// ---------------------------------------------------------------------------
// GCP auth flow
// ---------------------------------------------------------------------------

/// GCP-specific auth: check ADC credentials, optionally run gcloud login.
async fn run_gcp(
    config: &Config,
    check: bool,
    reconfigure: bool,
    non_interactive: bool,
) -> Result<(), ZuulError> {
    let project_id = config.project_id.as_deref().ok_or_else(|| {
        ZuulError::Config(
            "No project ID configured. Run 'zuul init' to set up your project.".to_string(),
        )
    })?;

    if !check {
        println!("Checking credentials for project '{project_id}'...");
    }

    let credentials = config.credentials.as_deref();

    let needs_setup = reconfigure || try_gcp_connect(project_id, credentials).await.is_err();

    if !needs_setup {
        if check {
            return Ok(());
        }
        let backend = try_gcp_connect(project_id, credentials).await?;
        return print_gcp_success(&backend, project_id, check).await;
    }

    if check {
        return Err(ZuulError::Auth("No valid credentials.".to_string()));
    }

    println!("No valid credentials found.\n");

    // Check for an existing SA key at the conventional path
    let auto_key_path = std::env::var("HOME").ok().map(|home| {
        PathBuf::from(&home)
            .join(".zuul")
            .join(format!("{project_id}-sa.json"))
    });
    let has_auto_key = auto_key_path.as_ref().is_some_and(|p| p.exists());

    println!("How would you like to authenticate?\n");
    println!("  1. Run 'gcloud auth application-default login' (single GCP account)");
    if has_auto_key {
        println!("  2. Use detected key: ~/.zuul/{project_id}-sa.json");
        println!("  3. Enter path to a different service account key file\n");
    } else {
        println!("  2. Configure a service account key file (multi-account setups)\n");
    }

    let default = if has_auto_key { "2" } else { "1" };
    let choice = prompt::input(&format!("Choice [{default}]"), non_interactive)?;
    let choice = choice.trim();
    let choice = if choice.is_empty() { default } else { choice };

    match choice {
        "2" if has_auto_key => {
            let key_str = auto_key_path.unwrap().to_string_lossy().to_string();
            configure_sa_key(&key_str, config)?;

            let backend = try_gcp_connect(project_id, Some(&key_str)).await?;
            print_gcp_success(&backend, project_id, false).await
        }
        "3" if has_auto_key => {
            let key_path = prompt::input("Path to service account key file", non_interactive)?;
            let key_path = key_path.trim();

            configure_sa_key(key_path, config)?;

            let backend = try_gcp_connect(project_id, Some(key_path)).await?;
            print_gcp_success(&backend, project_id, false).await
        }
        "2" => {
            let key_path = prompt::input("Path to service account key file", non_interactive)?;
            let key_path = key_path.trim();

            configure_sa_key(key_path, config)?;

            let backend = try_gcp_connect(project_id, Some(key_path)).await?;
            print_gcp_success(&backend, project_id, false).await
        }
        _ => {
            run_gcloud_login()?;

            let backend = try_gcp_connect(project_id, credentials).await?;
            print_gcp_success(&backend, project_id, false).await
        }
    }
}

/// Attempt to create a GCP client and verify connectivity.
async fn try_gcp_connect(
    project_id: &str,
    credentials: Option<&str>,
) -> Result<GcpBackend, ZuulError> {
    let client = GcpClient::new(project_id, credentials).await?;
    let backend = GcpBackend::new(client);
    backend.list_secrets(None).await?;
    Ok(backend)
}

/// Print GCP authentication success info.
async fn print_gcp_success(
    backend: &impl Backend,
    project_id: &str,
    check: bool,
) -> Result<(), ZuulError> {
    if check {
        return Ok(());
    }

    println!(
        "{} Authenticated to project '{project_id}'.",
        style("✔").green()
    );

    match backend.list_environments().await {
        Ok(envs) if envs.is_empty() => {
            println!("No environments configured yet.");
            println!("\nNext step:");
            println!("  terraform apply   # Create environments via Terraform");
        }
        Ok(envs) => {
            let names: Vec<&str> = envs.iter().map(|e| e.name.as_str()).collect();
            println!("Accessible environments: {}", names.join(", "));
            println!("\nReady to go! Try: zuul secret list --env {}", names[0]);
        }
        Err(_) => {
            println!("Credentials valid. Could not read environment list (limited permissions).");
        }
    }

    Ok(())
}

/// Configure a service account key file: validate it exists, test the
/// connection, and update `.zuul.toml` with the credentials path.
fn configure_sa_key(key_path: &str, config: &Config) -> Result<(), ZuulError> {
    let expanded = crate::config::expand_tilde(key_path);
    let path = PathBuf::from(&expanded);

    if !path.exists() {
        return Err(ZuulError::Auth(format!("Key file not found: '{expanded}'")));
    }

    // Validate it's parseable JSON with the expected fields
    let content = std::fs::read_to_string(&path)
        .map_err(|e| ZuulError::Auth(format!("Failed to read key file: {e}")))?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| ZuulError::Auth(format!("Key file is not valid JSON: {e}")))?;
    if json.get("type").and_then(|v| v.as_str()) != Some("service_account") {
        return Err(ZuulError::Auth(
            "Key file does not appear to be a service account key (missing \"type\": \"service_account\").".to_string()
        ));
    }

    // Update .zuul.toml with the credentials path
    if let Some(config_dir) = &config.config_dir {
        let config_path = config_dir.join(".zuul.toml");
        if config_path.exists() {
            let toml_content = std::fs::read_to_string(&config_path).unwrap_or_default();
            // Use the tilde form for the config file if the path is under HOME
            let config_value = if let Ok(home) = std::env::var("HOME") {
                expanded.replace(&home, "~")
            } else {
                expanded.clone()
            };

            let updated = if toml_content.contains("credentials") {
                // Replace existing credentials line
                toml_content
                    .lines()
                    .map(|line| {
                        if line.trim_start().starts_with("credentials") {
                            format!("credentials = \"{config_value}\"")
                        } else {
                            line.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n"
            } else {
                // Add credentials after project_id line
                toml_content
                    .lines()
                    .map(|line| {
                        if line.trim_start().starts_with("project_id") {
                            format!("{line}\ncredentials = \"{config_value}\"")
                        } else {
                            line.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n"
            };

            std::fs::write(&config_path, updated)
                .map_err(|e| ZuulError::Config(format!("Failed to update .zuul.toml: {e}")))?;
            println!(
                "{} Updated .zuul.toml with credentials path.",
                style("✔").green()
            );
        }
    }

    Ok(())
}

/// Invoke `gcloud auth application-default login` as a subprocess.
fn run_gcloud_login() -> Result<(), ZuulError> {
    println!("Launching gcloud authentication...\n");

    let status = ProcessCommand::new("gcloud")
        .args(["auth", "application-default", "login"])
        .status()
        .map_err(|e| {
            ZuulError::Auth(format!(
                "Failed to run 'gcloud auth application-default login': {e}. \
                 Is the Google Cloud SDK installed?"
            ))
        })?;

    if !status.success() {
        return Err(ZuulError::Auth(
            "gcloud authentication failed. Try running it manually.".to_string(),
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// File backend auth flow
// ---------------------------------------------------------------------------

/// File backend auth: validate the passphrase can decrypt the store.
fn run_file(config: &Config, check: bool) -> Result<(), ZuulError> {
    use crate::backend::file_backend::{DEFAULT_STORE_FILE, FileBackend};

    let config_dir = config.config_dir.as_deref().ok_or_else(|| {
        ZuulError::Config(
            "No .zuul.toml found. Run 'zuul init' to set up your project.".to_string(),
        )
    })?;

    let default_path = config_dir.join(DEFAULT_STORE_FILE);
    let store_path = config
        .file_path
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or(default_path);

    if !store_path.exists() {
        return Err(ZuulError::Config(format!(
            "Store file '{}' not found. Run 'zuul init --backend file' to create it.",
            store_path.display()
        )));
    }

    let identity = config.identity.as_ref().map(std::path::PathBuf::from);
    let backend = FileBackend::new(store_path, identity);

    // Try to read the store — this validates the passphrase.
    match tokio::runtime::Handle::current().block_on(backend.list_environments()) {
        Ok(envs) => {
            if check {
                return Ok(());
            }
            println!("{} Authentication valid.", style("✔").green());
            if envs.is_empty() {
                println!("No environments configured yet.");
                println!("\nNext step:");
                println!("  zuul env create <name>   # Create your first environment");
            } else {
                let names: Vec<&str> = envs.iter().map(|e| e.name.as_str()).collect();
                println!("Accessible environments: {}", names.join(", "));
                println!("\nReady to go! Try: zuul secret list --env {}", names[0]);
            }
            Ok(())
        }
        Err(e) => {
            if check {
                return Err(e);
            }
            Err(ZuulError::Auth(
                "Failed to decrypt store. Check your passphrase.".to_string(),
            ))
        }
    }
}
