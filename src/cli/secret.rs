use std::io::{IsTerminal, Read};
use std::path::Path;

use comfy_table::{ContentArrangement, Table};
use console::style;

use crate::backend::Backend;
use crate::cli::{OutputFormat, to_json_pretty};
use crate::error::ZuulError;
use crate::progress::{self, ProgressOpts};
use crate::prompt;

/// Resolve the target environment from config, returning an error if not set.
pub fn require_env(env: Option<&str>) -> Result<&str, ZuulError> {
    env.ok_or_else(|| {
        ZuulError::Config(
            "No environment specified. Use --env <name> or set a default in .zuul.toml."
                .to_string(),
        )
    })
}

/// Format a metadata map as `key=val, key2=val2` with sorted keys.
fn format_metadata(meta: &std::collections::HashMap<String, String>) -> String {
    if meta.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<_> = meta.iter().collect();
    pairs.sort_by_key(|(k, _)| *k);
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Run `zuul secret list`.
pub async fn list(
    backend: &impl Backend,
    env: Option<&str>,
    with_metadata: bool,
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

    if !with_metadata {
        match format {
            OutputFormat::Text => {
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                table.set_header(vec!["NAME", "ENVIRONMENTS"]);
                for secret in &secrets {
                    table.add_row(vec![secret.name.clone(), secret.environments.join(", ")]);
                }
                println!("{table}");
            }
            OutputFormat::Json => {
                let json = to_json_pretty(&secrets)?;
                println!("{json}");
            }
        }
        return Ok(());
    }

    // with_metadata is true — fetch metadata per secret per environment.
    let sp = progress::spinner("Fetching metadata...", progress);

    if let Some(environment) = env {
        // Single env: one METADATA column.
        let mut meta_per_secret: Vec<std::collections::HashMap<String, String>> =
            Vec::with_capacity(secrets.len());
        for secret in &secrets {
            meta_per_secret.push(backend.get_metadata(&secret.name, environment).await?);
        }
        sp.finish_and_clear();

        match format {
            OutputFormat::Text => {
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                table.set_header(vec!["NAME", "ENVIRONMENTS", "METADATA"]);
                for (secret, meta) in secrets.iter().zip(&meta_per_secret) {
                    table.add_row(vec![
                        secret.name.clone(),
                        secret.environments.join(", "),
                        format_metadata(meta),
                    ]);
                }
                println!("{table}");
            }
            OutputFormat::Json => {
                let entries: Vec<serde_json::Value> = secrets
                    .iter()
                    .zip(&meta_per_secret)
                    .map(|(secret, meta)| {
                        serde_json::json!({
                            "name": secret.name,
                            "environments": secret.environments,
                            "metadata": meta,
                        })
                    })
                    .collect();
                let json = to_json_pretty(&entries)?;
                println!("{json}");
            }
        }
    } else {
        // All envs: collect unique env names, one metadata column per env.
        let mut all_envs: Vec<String> = secrets
            .iter()
            .flat_map(|s| s.environments.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        all_envs.sort();

        // Fetch metadata for each (secret, env) pair.
        let mut meta_map: Vec<
            std::collections::HashMap<String, std::collections::HashMap<String, String>>,
        > = Vec::with_capacity(secrets.len());
        for secret in &secrets {
            let mut env_meta = std::collections::HashMap::new();
            for e in &secret.environments {
                let m = backend.get_metadata(&secret.name, e).await?;
                if !m.is_empty() {
                    env_meta.insert(e.clone(), m);
                }
            }
            meta_map.push(env_meta);
        }
        sp.finish_and_clear();

        match format {
            OutputFormat::Text => {
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                let mut header = vec!["NAME".to_string(), "ENVIRONMENTS".to_string()];
                header.extend(all_envs.iter().cloned());
                table.set_header(header);

                for (secret, env_meta) in secrets.iter().zip(&meta_map) {
                    let mut row = vec![secret.name.clone(), secret.environments.join(", ")];
                    for e in &all_envs {
                        row.push(env_meta.get(e).map(format_metadata).unwrap_or_default());
                    }
                    table.add_row(row);
                }
                println!("{table}");
            }
            OutputFormat::Json => {
                let entries: Vec<serde_json::Value> = secrets
                    .iter()
                    .zip(&meta_map)
                    .map(|(secret, env_meta)| {
                        serde_json::json!({
                            "name": secret.name,
                            "environments": secret.environments,
                            "metadata": env_meta,
                        })
                    })
                    .collect();
                let json = to_json_pretty(&entries)?;
                println!("{json}");
            }
        }
    }

    Ok(())
}

/// Run `zuul secret get`.
pub async fn get(
    backend: &impl Backend,
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
    backend: &impl Backend,
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
        prompt::password("Secret value", progress.non_interactive)?
    };

    let sp = progress::spinner(&format!("Setting secret '{name}'..."), progress);
    backend
        .set_secret(name, environment, &resolved_value)
        .await?;
    sp.finish_and_clear();

    if !progress.non_interactive {
        println!(
            "{} Set secret '{name}' in environment '{environment}'.",
            style("✔").green()
        );
    }

    Ok(())
}

/// Run `zuul secret delete`.
pub async fn delete(
    backend: &impl Backend,
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
                let json = to_json_pretty(&value)?;
                println!("{json}");
            }
        }
        return Ok(());
    }

    if !prompt::confirm(
        &format!("Delete secret '{name}' from environment '{environment}'?"),
        force,
        progress.non_interactive,
    )? {
        println!("Cancelled.");
        return Ok(());
    }

    let sp = progress::spinner(&format!("Deleting secret '{name}'..."), progress);
    backend.delete_secret(name, environment).await?;
    sp.finish_and_clear();

    match format {
        OutputFormat::Text => println!(
            "{} Deleted secret '{name}' from environment '{environment}'.",
            style("✔").green()
        ),
        OutputFormat::Json => {
            let value = serde_json::json!({
                "deleted": true,
                "secret": name,
                "environment": environment,
            });
            let json = to_json_pretty(&value)?;
            println!("{json}");
        }
    }

    Ok(())
}

/// Run `zuul secret info`.
pub async fn info(
    backend: &impl Backend,
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
            let json = to_json_pretty(&value)?;
            println!("{json}");
        }
    }

    Ok(())
}

/// Run `zuul secret copy`.
pub async fn copy(
    backend: &impl Backend,
    name: &str,
    from: &str,
    to: &str,
    force: bool,
    dry_run: bool,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let sp = progress::spinner(&format!("Copying secret '{name}'..."), progress);
    let _source = backend.get_secret(name, from).await?;

    // Check if the secret already exists in the target environment.
    let exists_in_target = backend.get_secret(name, to).await.is_ok();
    sp.finish_and_clear();

    if dry_run {
        if exists_in_target {
            println!(
                "Would copy secret '{name}' from '{from}' to '{to}' (already exists — would overwrite)."
            );
        } else {
            println!("Would copy secret '{name}' from '{from}' to '{to}'.");
        }
        println!("\nNo changes were made. Remove --dry-run to execute.");
        return Ok(());
    }

    if exists_in_target
        && !prompt::confirm(
            &format!("Secret '{name}' already exists in environment '{to}'. Overwrite?"),
            force,
            progress.non_interactive,
        )?
    {
        println!("Cancelled.");
        return Ok(());
    }

    backend.set_secret(name, to, &_source.value).await?;

    if !progress.non_interactive {
        println!(
            "{} Copied secret '{name}' from '{from}' to '{to}'.",
            style("✔").green()
        );
    }

    Ok(())
}
