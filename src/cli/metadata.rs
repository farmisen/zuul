use comfy_table::{ContentArrangement, Table};
use console::style;

use crate::backend::Backend;
use crate::cli::OutputFormat;
use crate::error::ZuulError;

/// Find all environments where a secret exists.
///
/// If `env` is provided, returns just that one (scoped operation).
/// Otherwise, looks up all environments via `list_secrets`.
async fn resolve_environments(
    backend: &impl Backend,
    name: &str,
    env: Option<&str>,
) -> Result<Vec<String>, ZuulError> {
    if let Some(e) = env {
        return Ok(vec![e.to_string()]);
    }

    let secrets = backend.list_secrets(None).await?;
    let entry = secrets
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| ZuulError::NotFound {
            resource_type: crate::error::ResourceType::Secret,
            name: name.to_string(),
            environment: None,
        })?;

    Ok(entry.environments.clone())
}

/// Run `zuul secret metadata list`.
///
/// With `--env`: show metadata for that single environment (KEY, VALUE columns).
/// Without `--env`: show metadata across all environments (KEY + one column per env).
pub async fn list(
    backend: &impl Backend,
    name: &str,
    env: Option<&str>,
    format: &OutputFormat,
) -> Result<(), ZuulError> {
    let envs = resolve_environments(backend, name, env).await?;

    if envs.len() == 1 {
        // Single env: flat KEY/VALUE display.
        let metadata = backend.get_metadata(name, &envs[0]).await?;

        if metadata.is_empty() {
            match format {
                OutputFormat::Text => println!("No metadata for secret '{name}'."),
                OutputFormat::Json => println!("{{}}"),
            }
            return Ok(());
        }

        match format {
            OutputFormat::Text => {
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                table.set_header(vec!["KEY", "VALUE"]);

                let mut pairs: Vec<_> = metadata.into_iter().collect();
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                for (k, v) in pairs {
                    table.add_row(vec![k, v]);
                }

                println!("{table}");
            }
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&metadata)
                    .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
                println!("{json}");
            }
        }
    } else {
        // All envs: one column per environment.
        let mut env_metadata: std::collections::BTreeMap<
            String,
            std::collections::HashMap<String, String>,
        > = std::collections::BTreeMap::new();
        for e in &envs {
            let m = backend.get_metadata(name, e).await?;
            env_metadata.insert(e.clone(), m);
        }

        let all_empty = env_metadata.values().all(|m| m.is_empty());
        if all_empty {
            match format {
                OutputFormat::Text => println!("No metadata for secret '{name}'."),
                OutputFormat::Json => println!("{{}}"),
            }
            return Ok(());
        }

        // Collect all unique keys across envs.
        let all_keys: Vec<String> = {
            let mut keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            for m in env_metadata.values() {
                keys.extend(m.keys().cloned());
            }
            keys.into_iter().collect()
        };

        let sorted_envs: Vec<&String> = env_metadata.keys().collect();

        match format {
            OutputFormat::Text => {
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                let mut header = vec!["KEY".to_string()];
                header.extend(sorted_envs.iter().map(|e| e.to_string()));
                table.set_header(header);

                for key in &all_keys {
                    let mut row = vec![key.clone()];
                    for e in &sorted_envs {
                        row.push(env_metadata[*e].get(key).cloned().unwrap_or_default());
                    }
                    table.add_row(row);
                }

                println!("{table}");
            }
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&env_metadata)
                    .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
                println!("{json}");
            }
        }
    }

    Ok(())
}

/// Run `zuul secret metadata set`.
///
/// Sets the metadata key across all environments where the secret exists.
/// If `--env` is provided, scopes to just that environment.
pub async fn set(
    backend: &impl Backend,
    name: &str,
    env: Option<&str>,
    key: &str,
    value: &str,
    non_interactive: bool,
) -> Result<(), ZuulError> {
    let envs = resolve_environments(backend, name, env).await?;

    for environment in &envs {
        backend.set_metadata(name, environment, key, value).await?;
    }

    if !non_interactive {
        if envs.len() == 1 {
            println!(
                "{} Set metadata '{key}' on secret '{name}' in environment '{}'.",
                style("✔").green(),
                envs[0]
            );
        } else {
            println!(
                "{} Set metadata '{key}' on secret '{name}' across {} environments.",
                style("✔").green(),
                envs.len()
            );
        }
    }

    Ok(())
}

/// Run `zuul secret metadata delete`.
///
/// Deletes the metadata key from all environments where the secret exists.
/// If `--env` is provided, scopes to just that environment.
pub async fn delete(
    backend: &impl Backend,
    name: &str,
    env: Option<&str>,
    key: &str,
    non_interactive: bool,
) -> Result<(), ZuulError> {
    let envs = resolve_environments(backend, name, env).await?;

    for environment in &envs {
        backend.delete_metadata(name, environment, key).await?;
    }

    if !non_interactive {
        if envs.len() == 1 {
            println!(
                "{} Deleted metadata '{key}' from secret '{name}' in environment '{}'.",
                style("✔").green(),
                envs[0]
            );
        } else {
            println!(
                "{} Deleted metadata '{key}' from secret '{name}' across {} environments.",
                style("✔").green(),
                envs.len()
            );
        }
    }

    Ok(())
}
