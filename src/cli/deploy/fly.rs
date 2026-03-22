use std::collections::HashMap;
use std::process::Command;

use console::style;

use crate::cli::sync::fly::FlyTarget;
use crate::cli::sync::{self, SyncTarget};
use crate::error::ZuulError;
use crate::progress::{self, ProgressOpts};

/// Run `zuul deploy fly`.
///
/// Fetches secrets for the given environment, optionally syncs them to
/// Fly's secret vault, then runs `fly deploy` with secrets injected as
/// environment variables.
pub fn run(
    secrets: HashMap<String, String>,
    app: Option<&str>,
    no_sync: bool,
    fly_args: &[String],
    progress: ProgressOpts,
) -> Result<i32, ZuulError> {
    // Sync secrets to Fly's vault unless --no-sync
    if !no_sync {
        let target = FlyTarget::new(app, true); // --stage: don't redeploy yet

        let sp = progress::spinner(
            &format!("Syncing secrets to {}...", target.target_description()),
            progress,
        );
        let platform_vars = target.list_vars()?;
        sp.finish_and_clear();

        let actions = sync::compute_diff(&secrets, &platform_vars, false);
        sync::execute_sync(&sync::SyncOpts {
            target: &target,
            actions: &actions,
            dry_run: false,
            prune: false,
            force: true,
            non_interactive: true,
        })?;
    }

    // Build the fly deploy command
    let mut args = vec!["deploy".to_string()];
    if let Some(app_name) = app {
        args.push("--app".to_string());
        args.push(app_name.to_string());
    }
    args.extend_from_slice(fly_args);

    // Build the child environment: current env + secrets, minus ZUUL_* vars
    let mut child_env: HashMap<String, String> = std::env::vars()
        .filter(|(k, _)| !k.starts_with("ZUUL_"))
        .collect();

    // Secrets override existing env vars
    let mut collision_count = 0;
    for (key, value) in &secrets {
        if child_env.contains_key(key) {
            collision_count += 1;
            eprintln!(
                "{}: env var '{key}' exists in current environment, secret value takes precedence",
                style("warning").yellow()
            );
        }
        child_env.insert(key.clone(), value.clone());
    }
    if collision_count > 0 {
        eprintln!();
    }

    println!(
        "Running: fly {}",
        args.iter()
            .map(|a| if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            })
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Spawn fly deploy
    let status = Command::new("fly")
        .args(&args)
        .envs(&child_env)
        .status()
        .map_err(|e| {
            ZuulError::Backend(format!(
                "Failed to run 'fly deploy': {e}. Is the Fly CLI installed?"
            ))
        })?;

    Ok(status.code().unwrap_or(1))
}
