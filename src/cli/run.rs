use std::collections::HashMap;
use std::process::Stdio;

use crate::backend::Backend;
use crate::backend::gcp_backend::GcpBackend;
use crate::config::Config;
use crate::error::ZuulError;
use crate::progress::{self, ProgressOpts};

/// Run `zuul run`.
///
/// Fetches all secrets for the given environment, applies local overrides
/// (unless `no_local` is set), merges them into the current process environment
/// (stripping `ZUUL_*` vars), spawns the child process, and returns its exit code.
pub async fn run(
    backend: &GcpBackend,
    config: &Config,
    env: &str,
    no_local: bool,
    command: &[String],
    progress: ProgressOpts,
) -> Result<i32, ZuulError> {
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

    // Apply local overrides unless --no-local
    if !no_local {
        for (key, value) in &config.local_overrides {
            secrets.insert(key.clone(), value.clone());
        }
    }

    // Build child environment: start with current process env, strip ZUUL_* vars
    let mut child_env: HashMap<String, String> = std::env::vars()
        .filter(|(k, _)| !k.starts_with("ZUUL_"))
        .collect();

    // Merge secrets into child env (secrets win on collision, warn on stderr)
    for (key, value) in &secrets {
        if let Some(existing) = child_env.get(key)
            && existing != value
        {
            eprintln!("Warning: secret '{key}' overrides existing environment variable");
        }
        child_env.insert(key.clone(), value.clone());
    }

    // Spawn child process
    let program = &command[0];
    let args = &command[1..];

    let status = std::process::Command::new(program)
        .args(args)
        .env_clear()
        .envs(&child_env)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| ZuulError::Backend(format!("Failed to execute '{program}': {e}")))?;

    Ok(status.code().unwrap_or(1))
}
