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
pub async fn run(config: &Config, check: bool, non_interactive: bool) -> Result<(), ZuulError> {
    match config.backend_type.as_str() {
        "gcp-secret-manager" => run_gcp(config, check, non_interactive).await,
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
async fn run_gcp(config: &Config, check: bool, non_interactive: bool) -> Result<(), ZuulError> {
    let project_id = config.project_id.as_deref().ok_or_else(|| {
        ZuulError::Config(
            "No project ID configured. Run 'zuul init' to set up your project.".to_string(),
        )
    })?;

    if !check {
        println!("Checking credentials for project '{project_id}'...");
    }

    let credentials = config.credentials.as_deref();

    match try_gcp_connect(project_id, credentials).await {
        Ok(backend) => print_gcp_success(&backend, project_id, check).await,
        Err(e) if check => Err(e),
        Err(_) => {
            println!("No valid credentials found.");

            if !prompt::confirm(
                "Run 'gcloud auth application-default login' now?",
                false,
                non_interactive,
            )? {
                return Err(ZuulError::Auth("Authentication required.".to_string()));
            }

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
