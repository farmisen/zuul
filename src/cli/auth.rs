use std::process::Command as ProcessCommand;

use dialoguer::Confirm;

use crate::backend::Backend;
use crate::backend::gcp::GcpClient;
use crate::backend::gcp_backend::GcpBackend;
use crate::config::Config;
use crate::error::ZuulError;

/// Run the `zuul auth` command.
///
/// In interactive mode, checks credentials and offers to run
/// `gcloud auth application-default login` if they are missing.
/// In `--check` mode, validates silently and returns an error on failure.
pub async fn run(config: &Config, check: bool) -> Result<(), ZuulError> {
    let project_id = config.project_id.as_deref().ok_or_else(|| {
        ZuulError::Config(
            "No GCP project ID configured. Run 'zuul init' to set up your project.".to_string(),
        )
    })?;

    if !check {
        println!("Checking credentials for GCP project '{project_id}'...");
    }

    let credentials = config.credentials.as_deref();

    match try_connect(project_id, credentials).await {
        Ok(backend) => print_success(&backend, project_id, check).await,
        Err(e) if check => Err(e),
        Err(_) => {
            println!("No valid credentials found.");

            let should_login = Confirm::new()
                .with_prompt("Run 'gcloud auth application-default login' now?")
                .default(true)
                .interact()
                .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))?;

            if !should_login {
                return Err(ZuulError::Auth(
                    "Authentication required. Run 'zuul auth' when ready.".to_string(),
                ));
            }

            run_gcloud_login()?;

            // Retry after login
            let backend = try_connect(project_id, credentials).await?;
            print_success(&backend, project_id, false).await
        }
    }
}

/// Attempt to create a GCP client and verify connectivity by listing secrets.
async fn try_connect(project_id: &str, credentials: Option<&str>) -> Result<GcpBackend, ZuulError> {
    let client = GcpClient::new(project_id, credentials).await?;
    let backend = GcpBackend::new(client);

    // Verify we can actually reach the project by listing secrets.
    backend.list_secrets(None).await?;

    Ok(backend)
}

/// Print authentication success info.
async fn print_success(
    backend: &GcpBackend,
    project_id: &str,
    check: bool,
) -> Result<(), ZuulError> {
    if check {
        return Ok(());
    }

    println!("Authenticated to GCP project '{project_id}'.");

    match backend.list_environments().await {
        Ok(envs) if envs.is_empty() => {
            println!("No environments configured yet.");
            println!("\nNext step:");
            println!("  zuul env create <name>   # Create your first environment");
        }
        Ok(envs) => {
            let names: Vec<&str> = envs.iter().map(|e| e.name.as_str()).collect();
            println!("Accessible environments: {}", names.join(", "));
            println!("\nReady to go! Try: zuul secret list --env {}", names[0]);
        }
        Err(_) => {
            // Could list secrets but not read registry — limited access
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

    println!();
    Ok(())
}
