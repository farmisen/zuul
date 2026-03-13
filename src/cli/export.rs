use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::backend::Backend;
use crate::backend::gcp_backend::GcpBackend;
use crate::cli::ExportFormat;
use crate::config::Config;
use crate::error::ZuulError;
use crate::export;

/// Run `zuul export`.
///
/// Fetches all secrets for the given environment, applies local overrides
/// (unless `no_local` is set), renders in the requested format, and writes
/// to the output file or stdout.
pub async fn run(
    backend: &GcpBackend,
    config: &Config,
    env: &str,
    format: &ExportFormat,
    output: Option<&Path>,
    no_local: bool,
) -> Result<(), ZuulError> {
    // Fetch all secrets for the environment
    let backend_secrets = backend.list_secrets_for_environment(env).await?;

    // Build name→value map from backend results
    let mut secrets: HashMap<String, String> = backend_secrets
        .into_iter()
        .map(|(name, sv)| (name, sv.value))
        .collect();

    // Apply local overrides unless --no-local
    if !no_local {
        for (key, value) in &config.local_overrides {
            secrets.insert(key.clone(), value.clone());
        }
    }

    // Sort by key for deterministic output
    let mut sorted: Vec<(String, String)> = secrets.into_iter().collect();
    sorted.sort_by(|(a, _), (b, _)| a.cmp(b));

    // Render
    let rendered = export::render(format, env, &sorted);

    // Write to file or stdout
    match output {
        Some(path) => {
            fs::write(path, &rendered).map_err(|e| {
                ZuulError::Config(format!("Failed to write to '{}': {e}", path.display()))
            })?;
        }
        None => {
            print!("{rendered}");
        }
    }

    Ok(())
}
