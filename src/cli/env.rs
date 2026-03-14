use comfy_table::{ContentArrangement, Table};
use dialoguer::Confirm;
use dialoguer::Input;

use crate::backend::Backend;
use crate::backend::gcp_backend::GcpBackend;
use crate::cli::OutputFormat;
use crate::error::ZuulError;
use crate::progress::{self, ProgressOpts};

/// Run `zuul env list`.
pub async fn list(backend: &GcpBackend, format: &OutputFormat) -> Result<(), ZuulError> {
    let envs = backend.list_environments().await?;

    if envs.is_empty() {
        match format {
            OutputFormat::Text => println!("No environments configured yet."),
            OutputFormat::Json => println!("[]"),
        }
        return Ok(());
    }

    match format {
        OutputFormat::Text => {
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec!["NAME", "DESCRIPTION", "SECRETS"]);

            for env in &envs {
                let count = backend.list_secrets(Some(&env.name)).await?.len();
                table.add_row(vec![
                    env.name.clone(),
                    env.description.clone().unwrap_or_default(),
                    count.to_string(),
                ]);
            }

            println!("{table}");
        }
        OutputFormat::Json => {
            let mut entries = Vec::new();
            for env in &envs {
                let count = backend.list_secrets(Some(&env.name)).await?.len();
                entries.push(serde_json::json!({
                    "name": env.name,
                    "description": env.description,
                    "secrets": count,
                }));
            }
            let json = serde_json::to_string_pretty(&entries)
                .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
            println!("{json}");
        }
    }

    Ok(())
}

/// Run `zuul env create`.
pub async fn create(
    backend: &GcpBackend,
    name: &str,
    description: Option<&str>,
    format: &OutputFormat,
) -> Result<(), ZuulError> {
    let env = backend.create_environment(name, description).await?;

    match format {
        OutputFormat::Text => {
            println!("Created environment '{}'.", env.name);
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&env)
                .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
            println!("{json}");
        }
    }

    Ok(())
}

/// Run `zuul env show`.
pub async fn show(
    backend: &GcpBackend,
    name: &str,
    format: &OutputFormat,
) -> Result<(), ZuulError> {
    let env = backend.get_environment(name).await?;
    let secret_count = backend.list_secrets(Some(name)).await?.len();

    match format {
        OutputFormat::Text => {
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);

            table.add_row(vec!["Name", &env.name]);
            table.add_row(vec![
                "Description",
                env.description.as_deref().unwrap_or("(none)"),
            ]);
            table.add_row(vec!["Secrets", &secret_count.to_string()]);
            table.add_row(vec![
                "Created",
                &env.created_at.format("%Y-%m-%d %H:%M").to_string(),
            ]);
            table.add_row(vec![
                "Updated",
                &env.updated_at.format("%Y-%m-%d %H:%M").to_string(),
            ]);

            println!("{table}");
        }
        OutputFormat::Json => {
            let value = serde_json::json!({
                "name": env.name,
                "description": env.description,
                "secret_count": secret_count,
                "created_at": env.created_at,
                "updated_at": env.updated_at,
            });
            let json = serde_json::to_string_pretty(&value)
                .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
            println!("{json}");
        }
    }

    Ok(())
}

/// Run `zuul env update`.
pub async fn update(
    backend: &GcpBackend,
    name: &str,
    new_name: Option<&str>,
    new_description: Option<&str>,
    format: &OutputFormat,
) -> Result<(), ZuulError> {
    if new_name.is_none() && new_description.is_none() {
        return Err(ZuulError::Validation(
            "Nothing to update. Provide --new-name and/or --description.".to_string(),
        ));
    }

    // Renaming requires confirmation since it renames N GCP secrets.
    if let Some(target) = new_name {
        let secrets = backend.list_secrets(Some(name)).await?;
        if !secrets.is_empty() {
            println!(
                "Renaming '{name}' to '{target}' will rename {} secret(s).",
                secrets.len()
            );
            let confirmed = Confirm::new()
                .with_prompt("Continue?")
                .default(false)
                .interact()
                .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))?;

            if !confirmed {
                println!("Cancelled.");
                return Ok(());
            }
        }
    }

    let env = backend
        .update_environment(name, new_name, new_description)
        .await?;

    match format {
        OutputFormat::Text => {
            println!("Updated environment '{}'.", env.name);
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&env)
                .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
            println!("{json}");
        }
    }

    Ok(())
}

/// Run `zuul env delete`.
pub async fn delete(
    backend: &GcpBackend,
    name: &str,
    dry_run: bool,
    format: &OutputFormat,
) -> Result<(), ZuulError> {
    // Verify environment exists before showing confirmation.
    backend.get_environment(name).await?;

    let secrets = backend.list_secrets(Some(name)).await?;

    // Recap what will be deleted.
    match format {
        OutputFormat::Text => {
            if dry_run {
                print!("Dry run: the following");
            } else {
                print!("The following");
            }
            println!(" resources would be deleted:\n");
            println!("  Environment: {name}");
            if secrets.is_empty() {
                println!("  Secrets: (none)");
            } else {
                println!("  Secrets ({}):", secrets.len());
                for secret in &secrets {
                    println!("    - zuul__{name}__{}", secret.name);
                }
            }

            if dry_run {
                println!("\nNo changes were made. Remove --dry-run to execute.");
                return Ok(());
            }
        }
        OutputFormat::Json => {
            let secret_names: Vec<&str> = secrets.iter().map(|s| s.name.as_str()).collect();
            let value = serde_json::json!({
                "environment": name,
                "secrets": secret_names,
                "dry_run": dry_run,
            });
            if dry_run {
                let json = serde_json::to_string_pretty(&value)
                    .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
                println!("{json}");
                return Ok(());
            }
        }
    }

    // Step 1: Confirm yes/no
    println!();
    let confirmed = Confirm::new()
        .with_prompt("Are you sure you want to delete this environment?")
        .default(false)
        .interact()
        .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    // Step 2: Type "delete <name>" to confirm
    let expected = format!("delete {name}");
    let typed: String = Input::new()
        .with_prompt(format!("Type '{expected}' to confirm"))
        .interact_text()
        .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))?;

    if typed != expected {
        println!("Confirmation did not match. Cancelled.");
        return Ok(());
    }

    backend.delete_environment(name).await?;

    match format {
        OutputFormat::Text => {
            let count = secrets.len();
            if count > 0 {
                println!("Deleted environment '{name}' and {count} secret(s).");
            } else {
                println!("Deleted environment '{name}'.");
            }
        }
        OutputFormat::Json => {
            let value = serde_json::json!({
                "deleted": true,
                "environment": name,
                "secrets_deleted": secrets.len(),
            });
            let json = serde_json::to_string_pretty(&value)
                .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
            println!("{json}");
        }
    }

    Ok(())
}

/// Run `zuul env clear`.
///
/// Deletes all secrets in an environment but keeps the environment itself.
pub async fn clear(
    backend: &GcpBackend,
    name: &str,
    force: bool,
    dry_run: bool,
    format: &OutputFormat,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    // Verify environment exists.
    backend.get_environment(name).await?;

    let sp = progress::spinner("Fetching secrets...", progress);
    let secrets = backend.list_secrets(Some(name)).await?;
    sp.finish_and_clear();

    if secrets.is_empty() {
        match format {
            OutputFormat::Text => println!("No secrets found in environment '{name}'."),
            OutputFormat::Json => {
                let value = serde_json::json!({
                    "environment": name,
                    "secrets_deleted": 0,
                    "dry_run": dry_run,
                });
                let json = serde_json::to_string_pretty(&value)
                    .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
                println!("{json}");
            }
        }
        return Ok(());
    }

    // Show what will be cleared.
    match format {
        OutputFormat::Text => {
            if dry_run {
                print!("Dry run: the following");
            } else {
                print!("The following");
            }
            println!(" secrets would be deleted from environment '{name}':\n");
            for secret in &secrets {
                println!("  - {}", secret.name);
            }

            if dry_run {
                println!("\nNo changes were made. Remove --dry-run to execute.");
                return Ok(());
            }
        }
        OutputFormat::Json => {
            let secret_names: Vec<&str> = secrets.iter().map(|s| s.name.as_str()).collect();
            let value = serde_json::json!({
                "environment": name,
                "secrets": secret_names,
                "dry_run": dry_run,
            });
            if dry_run {
                let json = serde_json::to_string_pretty(&value)
                    .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
                println!("{json}");
                return Ok(());
            }
        }
    }

    if !force {
        println!();
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Delete all {} secret(s) from environment '{name}'?",
                secrets.len()
            ))
            .default(false)
            .interact()
            .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))?;

        if !confirmed {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let pb = progress::progress_bar(secrets.len() as u64, progress);
    for secret in &secrets {
        pb.set_message(format!("Deleting '{}'...", secret.name));
        backend.delete_secret(&secret.name, name).await?;
        pb.inc(1);
    }
    pb.finish_and_clear();

    match format {
        OutputFormat::Text => {
            println!(
                "Cleared {} secret(s) from environment '{name}'.",
                secrets.len()
            );
        }
        OutputFormat::Json => {
            let value = serde_json::json!({
                "cleared": true,
                "environment": name,
                "secrets_deleted": secrets.len(),
            });
            let json = serde_json::to_string_pretty(&value)
                .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
            println!("{json}");
        }
    }

    Ok(())
}
