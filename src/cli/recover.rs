use std::path::Path;

use console::style;

use crate::backend::Backend;
use crate::error::ZuulError;
use crate::journal::{self, Journal, OperationType, StepStatus};
use crate::progress::{self, ProgressOpts};
use crate::prompt;

/// Fallback when a journal param is missing.
const UNKNOWN_PARAM: &str = "?";

/// Human-readable label for an operation type.
fn operation_label(op: &OperationType) -> &'static str {
    match op {
        OperationType::Import => "import",
        OperationType::EnvClear => "env clear",
        OperationType::EnvCopy => "env copy",
        OperationType::MetadataSet => "metadata set",
        OperationType::MetadataDelete => "metadata delete",
    }
}

/// Format journal params as a short summary string.
fn params_summary(journal: &Journal) -> String {
    match journal.operation {
        OperationType::Import => {
            let env = journal.params["environment"].as_str().unwrap_or(UNKNOWN_PARAM);
            let file = journal.params["file"].as_str().unwrap_or(UNKNOWN_PARAM);
            format!("environment '{env}', file '{file}'")
        }
        OperationType::EnvClear => {
            let env = journal.params["environment"].as_str().unwrap_or(UNKNOWN_PARAM);
            format!("environment '{env}'")
        }
        OperationType::EnvCopy => {
            let from = journal.params["from"].as_str().unwrap_or(UNKNOWN_PARAM);
            let to = journal.params["to"].as_str().unwrap_or(UNKNOWN_PARAM);
            format!("from '{from}' to '{to}'")
        }
        OperationType::MetadataSet => {
            let secret = journal.params["secret"].as_str().unwrap_or(UNKNOWN_PARAM);
            let key = journal.params["key"].as_str().unwrap_or(UNKNOWN_PARAM);
            format!("secret '{secret}', key '{key}'")
        }
        OperationType::MetadataDelete => {
            let secret = journal.params["secret"].as_str().unwrap_or(UNKNOWN_PARAM);
            let key = journal.params["key"].as_str().unwrap_or(UNKNOWN_PARAM);
            format!("secret '{secret}', key '{key}'")
        }
    }
}

/// Load the journal or print "no incomplete operations" and return Ok(None).
fn load_or_none(project_root: &Path) -> Result<Option<Journal>, ZuulError> {
    let jrnl = journal::load_journal(project_root)?;
    if jrnl.is_none() {
        println!("No incomplete operations found.");
    }
    Ok(jrnl)
}

/// Run `zuul recover status`.
pub fn status(project_root: &Path) -> Result<(), ZuulError> {
    let Some(jrnl) = load_or_none(project_root)? else {
        return Ok(());
    };

    let completed = jrnl.completed_count();
    let total = jrnl.steps.len();
    let label = operation_label(&jrnl.operation);
    let summary = params_summary(&jrnl);

    println!("Incomplete operation found: {label} ({summary})");
    println!(
        "Started: {}",
        jrnl.started_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!("Progress: {completed} of {total} steps completed\n");

    // List pending steps.
    let pending: Vec<_> = jrnl
        .steps
        .iter()
        .filter(|s| s.status == StepStatus::Pending)
        .collect();

    if !pending.is_empty() {
        println!("Pending steps:");
        for step in &pending {
            let target = step.target.as_deref().unwrap_or("(none)");
            println!("  - {} {}", step.action, target);
        }
    }

    println!("\nRun 'zuul recover resume' to continue, or 'zuul recover abort' to discard.");

    Ok(())
}

/// Run `zuul recover resume`.
pub async fn resume(
    backend: &impl Backend,
    project_root: &Path,
    force: bool,
    non_interactive: bool,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let Some(mut jrnl) = load_or_none(project_root)? else {
        return Ok(());
    };

    if jrnl.is_complete() {
        println!("All steps already completed. Cleaning up journal.");
        journal::delete_journal(project_root)?;
        return Ok(());
    }

    let completed = jrnl.completed_count();
    let total = jrnl.steps.len();
    let label = operation_label(&jrnl.operation);
    let summary = params_summary(&jrnl);

    println!("Resuming {label} ({summary}) — {completed} of {total} steps completed.");

    if !prompt::confirm("Continue?", force, non_interactive)? {
        println!("Cancelled.");
        return Ok(());
    }

    let pending_count = total - completed;
    let pb = progress::progress_bar(pending_count as u64, progress);

    for (i, step) in jrnl.steps.clone().iter().enumerate() {
        if step.status == StepStatus::Completed {
            continue;
        }

        let target = step.target.as_deref().unwrap_or("");
        pb.set_message(format!("{} '{target}'...", step.action));

        execute_step(backend, &jrnl, &step.action, target).await?;

        jrnl.mark_completed(i);
        journal::save_journal(project_root, &jrnl)?;
        pb.inc(1);
    }

    pb.finish_and_clear();
    journal::delete_journal(project_root)?;

    println!(
        "{} Resumed and completed {label} ({summary}).",
        style("✔").green()
    );

    Ok(())
}

/// Execute a single journal step against the backend.
async fn execute_step(
    backend: &impl Backend,
    journal: &Journal,
    action: &str,
    target: &str,
) -> Result<(), ZuulError> {
    match (journal.operation.clone(), action) {
        // Import: set_secret in the target environment.
        (OperationType::Import, "set_secret") => {
            let env = journal.params["environment"]
                .as_str()
                .ok_or_else(|| ZuulError::Config("Journal missing 'environment' param".into()))?;
            let file_path = journal.params["file"]
                .as_str()
                .ok_or_else(|| ZuulError::Config("Journal missing 'file' param".into()))?;

            let content = std::fs::read_to_string(file_path).map_err(|e| {
                ZuulError::Config(format!(
                    "Cannot read source file '{file_path}' for resume: {e}. \
                     If the file was moved or deleted, restore it and retry."
                ))
            })?;

            // Parse the file to find the value for this secret.
            let secrets = crate::cli::import::parse_auto(&content, file_path)?;
            let value = secrets
                .iter()
                .find(|(name, _)| name == target)
                .map(|(_, v)| v.as_str())
                .ok_or_else(|| {
                    ZuulError::Config(format!(
                        "Secret '{target}' not found in source file '{file_path}'"
                    ))
                })?;

            backend.set_secret(target, env, value).await
        }

        // EnvClear: delete_secret from the environment.
        (OperationType::EnvClear, "delete_secret") => {
            let env = journal.params["environment"]
                .as_str()
                .ok_or_else(|| ZuulError::Config("Journal missing 'environment' param".into()))?;
            backend.delete_secret(target, env).await
        }

        // EnvCopy: set_secret in the target environment (re-fetch from source).
        (OperationType::EnvCopy, "set_secret") => {
            let from = journal.params["from"]
                .as_str()
                .ok_or_else(|| ZuulError::Config("Journal missing 'from' param".into()))?;
            let to = journal.params["to"]
                .as_str()
                .ok_or_else(|| ZuulError::Config("Journal missing 'to' param".into()))?;
            let sv = backend.get_secret(target, from).await?;
            backend.set_secret(target, to, &sv.value).await
        }

        // MetadataSet: set metadata on the target environment.
        (OperationType::MetadataSet, "set_metadata") => {
            let secret = journal.params["secret"]
                .as_str()
                .ok_or_else(|| ZuulError::Config("Journal missing 'secret' param".into()))?;
            let key = journal.params["key"]
                .as_str()
                .ok_or_else(|| ZuulError::Config("Journal missing 'key' param".into()))?;
            let value = journal.params["value"]
                .as_str()
                .ok_or_else(|| ZuulError::Config("Journal missing 'value' param".into()))?;
            backend.set_metadata(secret, target, key, value).await
        }

        // MetadataDelete: delete metadata from the target environment.
        (OperationType::MetadataDelete, "delete_metadata") => {
            let secret = journal.params["secret"]
                .as_str()
                .ok_or_else(|| ZuulError::Config("Journal missing 'secret' param".into()))?;
            let key = journal.params["key"]
                .as_str()
                .ok_or_else(|| ZuulError::Config("Journal missing 'key' param".into()))?;
            backend.delete_metadata(secret, target, key).await
        }

        _ => Err(ZuulError::Config(format!(
            "Unknown journal step: operation={:?}, action='{action}'",
            journal.operation
        ))),
    }
}

/// Run `zuul recover abort`.
pub fn abort(project_root: &Path, force: bool, non_interactive: bool) -> Result<(), ZuulError> {
    let Some(jrnl) = load_or_none(project_root)? else {
        return Ok(());
    };

    let completed = jrnl.completed_count();
    let total = jrnl.steps.len();
    let label = operation_label(&jrnl.operation);
    let summary = params_summary(&jrnl);

    println!("Aborting {label} ({summary}) — {completed} of {total} steps were completed.");

    // Show what was left incomplete.
    let pending: Vec<_> = jrnl
        .steps
        .iter()
        .filter(|s| s.status == StepStatus::Pending)
        .collect();

    if !pending.is_empty() {
        println!("\nThe following steps were NOT completed:");
        for step in &pending {
            let target = step.target.as_deref().unwrap_or("(none)");
            println!("  - {} {}", step.action, target);
        }
        println!("\nYou may need to clean up this partial state manually.");
    }

    if !prompt::confirm("Discard the journal?", force, non_interactive)? {
        println!("Cancelled.");
        return Ok(());
    }

    journal::delete_journal(project_root)?;
    println!("{} Journal discarded.", style("✔").green());

    Ok(())
}
