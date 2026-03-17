use comfy_table::{ContentArrangement, Table};
use console::style;

use crate::backend::Backend;
use crate::cli::OutputFormat;
use crate::error::ZuulError;
use crate::progress::{self, ProgressOpts};
use crate::prompt;

/// Run `zuul env list`.
pub async fn list(backend: &impl Backend, format: &OutputFormat) -> Result<(), ZuulError> {
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

/// Run `zuul env show`.
pub async fn show(
    backend: &impl Backend,
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

/// Run `zuul env copy`.
///
/// Copies all secrets from one environment to another. Both environments must
/// exist. Secrets already present in the target are overwritten; secrets only
/// in the target are left untouched.
pub async fn copy(
    backend: &impl Backend,
    from: &str,
    to: &str,
    force: bool,
    dry_run: bool,
    format: &OutputFormat,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    // Verify both environments exist.
    backend.get_environment(from).await?;
    backend.get_environment(to).await?;

    // Fetch source secrets (name + value).
    let sp = progress::spinner("Fetching secrets...", progress);
    let source_secrets = backend.list_secrets_for_environment(from).await?;
    let target_secrets = backend.list_secrets(Some(to)).await?;
    sp.finish_and_clear();

    if source_secrets.is_empty() {
        match format {
            OutputFormat::Text => println!("No secrets found in environment '{from}'."),
            OutputFormat::Json => {
                let value = serde_json::json!({
                    "from": from,
                    "to": to,
                    "copied": 0,
                    "overwritten": 0,
                    "dry_run": dry_run,
                });
                let json = serde_json::to_string_pretty(&value)
                    .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
                println!("{json}");
            }
        }
        return Ok(());
    }

    // Determine which secrets will be overwritten.
    let target_names: std::collections::HashSet<&str> =
        target_secrets.iter().map(|s| s.name.as_str()).collect();
    let overwrite_count = source_secrets
        .iter()
        .filter(|(name, _)| target_names.contains(name.as_str()))
        .count();
    let new_count = source_secrets.len() - overwrite_count;

    // Show preview.
    match format {
        OutputFormat::Text => {
            if dry_run {
                print!("Dry run: the following");
            } else {
                print!("The following");
            }
            println!(" secrets would be copied from '{from}' to '{to}':\n");
            for (name, _) in &source_secrets {
                let marker = if target_names.contains(name.as_str()) {
                    " (overwrite)"
                } else {
                    ""
                };
                println!("  - {name}{marker}");
            }
            println!("\n  {} new, {} overwrite", new_count, overwrite_count);

            if dry_run {
                println!("\nNo changes were made. Remove --dry-run to execute.");
                return Ok(());
            }
        }
        OutputFormat::Json => {
            let secret_names: Vec<&str> = source_secrets.iter().map(|(n, _)| n.as_str()).collect();
            let value = serde_json::json!({
                "from": from,
                "to": to,
                "secrets": secret_names,
                "new": new_count,
                "overwritten": overwrite_count,
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

    if !prompt::confirm(
        &format!(
            "Copy {} secret(s) from '{from}' to '{to}'?",
            source_secrets.len()
        ),
        force,
        progress.non_interactive,
    )? {
        println!("Cancelled.");
        return Ok(());
    }

    let pb = progress::progress_bar(source_secrets.len() as u64, progress);
    for (name, sv) in &source_secrets {
        pb.set_message(format!("Copying '{name}'..."));
        backend.set_secret(name, to, &sv.value).await?;
        pb.inc(1);
    }
    pb.finish_and_clear();

    match format {
        OutputFormat::Text => {
            println!(
                "{} Copied {} secret(s) from '{from}' to '{to}' ({} new, {} overwritten).",
                style("✔").green(),
                source_secrets.len(),
                new_count,
                overwrite_count
            );
        }
        OutputFormat::Json => {
            let value = serde_json::json!({
                "copied": true,
                "from": from,
                "to": to,
                "total": source_secrets.len(),
                "new": new_count,
                "overwritten": overwrite_count,
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
    backend: &impl Backend,
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

    if !prompt::confirm(
        &format!(
            "Delete all {} secret(s) from environment '{name}'?",
            secrets.len()
        ),
        force,
        progress.non_interactive,
    )? {
        println!("Cancelled.");
        return Ok(());
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
                "{} Cleared {} secret(s) from environment '{name}'.",
                style("✔").green(),
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
