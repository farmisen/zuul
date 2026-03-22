use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::backend::Backend;
use crate::cli::ExportFormat;
use crate::config::Config;
use crate::error::ZuulError;
use crate::export;
use crate::progress::{self, ProgressOpts};

/// Run `zuul export`.
///
/// Fetches all secrets for the given environment, applies local overrides
/// (if `overrides` is set), renders in the requested format, and writes
/// to the output file or stdout.
pub async fn run(
    backend: &impl Backend,
    config: &Config,
    env: &str,
    format: &ExportFormat,
    output: Option<&Path>,
    overrides: bool,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    // Verify environment exists
    backend.get_environment(env).await?;

    // Fetch all secrets for the environment
    let sp = progress::spinner("Fetching secrets...", progress);
    let backend_secrets = backend.list_secrets_for_environment(env).await?;
    sp.finish_and_clear();

    // Build name→value map from backend results
    let mut secrets: HashMap<String, String> = backend_secrets
        .into_iter()
        .map(|(name, sv)| (name, sv.value))
        .collect();

    // Apply local overrides if --overrides
    if overrides {
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
