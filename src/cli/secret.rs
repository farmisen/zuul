use std::io::{IsTerminal, Read};
use std::path::Path;

use comfy_table::{ContentArrangement, Table};
use dialoguer::{Confirm, Password};

use crate::backend::Backend;
use crate::backend::gcp_backend::GcpBackend;
use crate::cli::OutputFormat;
use crate::error::ZuulError;
use crate::progress::{self, ProgressOpts};

/// Resolve the target environment from config, returning an error if not set.
pub fn require_env(env: Option<&str>) -> Result<&str, ZuulError> {
    env.ok_or_else(|| {
        ZuulError::Config(
            "No environment specified. Use --env <name> or set a default in .zuul.toml."
                .to_string(),
        )
    })
}

/// Run `zuul secret list`.
pub async fn list(
    backend: &GcpBackend,
    env: Option<&str>,
    format: &OutputFormat,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let sp = progress::spinner("Fetching secrets...", progress);
    let secrets = backend.list_secrets(env).await?;
    sp.finish_and_clear();

    if secrets.is_empty() {
        match format {
            OutputFormat::Text => println!("No secrets found."),
            OutputFormat::Json => println!("[]"),
        }
        return Ok(());
    }

    match format {
        OutputFormat::Text => {
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);

            if let Some(environment) = env {
                // Per-environment mode: show NAME + UPDATED
                table.set_header(vec!["NAME", "UPDATED"]);
                let pb = progress::progress_bar(secrets.len() as u64, progress);
                for secret in &secrets {
                    pb.set_message(secret.name.clone());
                    let updated = match backend.get_secret(&secret.name, environment).await {
                        Ok(sv) => sv.updated_at.format("%Y-%m-%d %H:%M").to_string(),
                        Err(_) => "(unknown)".to_string(),
                    };
                    table.add_row(vec![secret.name.clone(), updated]);
                    pb.inc(1);
                }
                pb.finish_and_clear();
            } else {
                // Cross-environment mode: show NAME + ENVIRONMENTS
                table.set_header(vec!["NAME", "ENVIRONMENTS"]);
                for secret in &secrets {
                    table.add_row(vec![secret.name.clone(), secret.environments.join(", ")]);
                }
            }

            println!("{table}");
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&secrets)
                .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
            println!("{json}");
        }
    }

    Ok(())
}

/// Run `zuul secret get`.
pub async fn get(
    backend: &GcpBackend,
    name: &str,
    env: Option<&str>,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let environment = require_env(env)?;
    let sp = progress::spinner("Fetching secret...", progress);
    let secret = backend.get_secret(name, environment).await?;
    sp.finish_and_clear();
    print!("{}", secret.value);
    if std::io::stdout().is_terminal() {
        println!();
    }
    Ok(())
}

/// Run `zuul secret set`.
pub async fn set(
    backend: &GcpBackend,
    name: &str,
    env: Option<&str>,
    value: Option<&str>,
    from_file: Option<&Path>,
    from_stdin: bool,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let environment = require_env(env)?;

    let resolved_value = if let Some(path) = from_file {
        std::fs::read_to_string(path).map_err(|e| {
            ZuulError::Config(format!("Failed to read file '{}': {e}", path.display()))
        })?
    } else if from_stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| ZuulError::Config(format!("Failed to read stdin: {e}")))?;
        buf
    } else if let Some(v) = value {
        v.to_string()
    } else {
        Password::new()
            .with_prompt("Secret value")
            .interact()
            .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))?
    };

    let sp = progress::spinner(&format!("Setting secret '{name}'..."), progress);
    backend
        .set_secret(name, environment, &resolved_value)
        .await?;
    sp.finish_and_clear();

    if !progress.quiet {
        println!("Set secret '{name}' in environment '{environment}'.");
    }

    Ok(())
}

/// Run `zuul secret delete`.
pub async fn delete(
    backend: &GcpBackend,
    name: &str,
    env: Option<&str>,
    force: bool,
    dry_run: bool,
    format: &OutputFormat,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let environment = require_env(env)?;

    // Verify the secret exists.
    backend.get_secret(name, environment).await?;

    let gcp_name = format!("zuul__{environment}__{name}");

    if dry_run {
        match format {
            OutputFormat::Text => {
                println!("Dry run: would delete secret '{name}' ({gcp_name}).");
                println!("\nNo changes were made. Remove --dry-run to execute.");
            }
            OutputFormat::Json => {
                let value = serde_json::json!({
                    "secret": name,
                    "environment": environment,
                    "gcp_name": gcp_name,
                    "dry_run": true,
                });
                let json = serde_json::to_string_pretty(&value)
                    .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
                println!("{json}");
            }
        }
        return Ok(());
    }

    if !force {
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Delete secret '{name}' from environment '{environment}'?"
            ))
            .default(false)
            .interact()
            .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))?;

        if !confirmed {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let sp = progress::spinner(&format!("Deleting secret '{name}'..."), progress);
    backend.delete_secret(name, environment).await?;
    sp.finish_and_clear();

    match format {
        OutputFormat::Text => println!("Deleted secret '{name}' from environment '{environment}'."),
        OutputFormat::Json => {
            let value = serde_json::json!({
                "deleted": true,
                "secret": name,
                "environment": environment,
            });
            let json = serde_json::to_string_pretty(&value)
                .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
            println!("{json}");
        }
    }

    Ok(())
}

/// Run `zuul secret info`.
pub async fn info(
    backend: &GcpBackend,
    name: &str,
    env: Option<&str>,
    format: &OutputFormat,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    // List all secrets to find which environments have this secret.
    let sp = progress::spinner("Fetching secret info...", progress);
    let all_secrets = backend.list_secrets(None).await?;
    sp.finish_and_clear();
    let entry = all_secrets
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| ZuulError::NotFound {
            resource_type: crate::error::ResourceType::Secret,
            name: name.to_string(),
            environment: None,
        })?;

    // If --env is specified, show metadata for that specific environment.
    let metadata = if let Some(environment) = env {
        Some(backend.get_metadata(name, environment).await?)
    } else {
        None
    };

    match format {
        OutputFormat::Text => {
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);

            table.add_row(vec!["Name", name]);
            table.add_row(vec!["Environments", &entry.environments.join(", ")]);

            if let Some(ref meta) = metadata {
                if meta.is_empty() {
                    table.add_row(vec!["Metadata", "(none)"]);
                } else {
                    let mut pairs: Vec<_> = meta.iter().collect();
                    pairs.sort_by_key(|(k, _)| *k);
                    let first = pairs.remove(0);
                    table.add_row(vec![
                        "Metadata".to_string(),
                        format!("{} = {}", first.0, first.1),
                    ]);
                    for (k, v) in pairs {
                        table.add_row(vec![String::new(), format!("{k} = {v}")]);
                    }
                }
            }

            println!("{table}");
        }
        OutputFormat::Json => {
            let mut value = serde_json::json!({
                "name": name,
                "environments": entry.environments,
            });
            if let Some(meta) = metadata {
                value["metadata"] = serde_json::json!(meta);
            }
            let json = serde_json::to_string_pretty(&value)
                .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
            println!("{json}");
        }
    }

    Ok(())
}

/// Run `zuul secret copy`.
pub async fn copy(
    backend: &GcpBackend,
    name: &str,
    from: &str,
    to: &str,
    force: bool,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let sp = progress::spinner(&format!("Copying secret '{name}'..."), progress);
    let source = backend.get_secret(name, from).await?;

    // Check if the secret already exists in the target environment.
    let exists_in_target = backend.get_secret(name, to).await.is_ok();

    if exists_in_target && !force {
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Secret '{name}' already exists in environment '{to}'. Overwrite?"
            ))
            .default(false)
            .interact()
            .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))?;

        if !confirmed {
            println!("Cancelled.");
            return Ok(());
        }
    }

    backend.set_secret(name, to, &source.value).await?;
    sp.finish_and_clear();

    if !progress.quiet {
        println!("Copied secret '{name}' from '{from}' to '{to}'.");
    }

    Ok(())
}
