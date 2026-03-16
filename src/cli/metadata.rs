use comfy_table::{ContentArrangement, Table};
use console::style;

use crate::backend::Backend;
use crate::backend::gcp_backend::GcpBackend;
use crate::cli::OutputFormat;
use crate::error::ZuulError;

/// Find all environments where a secret exists.
///
/// If `env` is provided, returns just that one (scoped operation).
/// Otherwise, looks up all environments via `list_secrets`.
async fn resolve_environments(
    backend: &GcpBackend,
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
/// Metadata is shared across environments, so `--env` is optional.
/// If not provided, reads from the first environment where the secret exists.
pub async fn list(
    backend: &GcpBackend,
    name: &str,
    env: Option<&str>,
    format: &OutputFormat,
) -> Result<(), ZuulError> {
    let envs = resolve_environments(backend, name, env).await?;
    let environment = &envs[0];
    let metadata = backend.get_metadata(name, environment).await?;

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

    Ok(())
}

/// Run `zuul secret metadata set`.
///
/// Sets the metadata key across all environments where the secret exists.
/// If `--env` is provided, scopes to just that environment.
pub async fn set(
    backend: &GcpBackend,
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
    backend: &GcpBackend,
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
