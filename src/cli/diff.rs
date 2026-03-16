use std::collections::BTreeSet;

use comfy_table::{ContentArrangement, Table};
use console::style;

use crate::backend::Backend;
use crate::cli::OutputFormat;
use crate::error::ZuulError;
use crate::progress::{self, ProgressOpts};

/// Status of a secret in a diff comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffStatus {
    /// Present in both environments with the same value.
    Equal,
    /// Present in both environments with different values.
    Differs,
    /// Present only in environment A.
    OnlyInA,
    /// Present only in environment B.
    OnlyInB,
}

/// A single row in the diff output.
#[derive(Debug, Clone)]
struct DiffEntry {
    name: String,
    status: DiffStatus,
    value_a: Option<String>,
    value_b: Option<String>,
}

/// Run `zuul diff <env_a> <env_b>`.
pub async fn run(
    backend: &impl Backend,
    env_a: &str,
    env_b: &str,
    show_values: bool,
    format: &OutputFormat,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    // Validate both environments exist
    backend.get_environment(env_a).await?;
    backend.get_environment(env_b).await?;

    // Fetch secret lists for both environments
    let sp = progress::spinner("Fetching secrets...", progress);
    let secrets_a = backend.list_secrets(Some(env_a)).await?;
    let secrets_b = backend.list_secrets(Some(env_b)).await?;
    sp.finish_and_clear();

    // Collect all unique secret names
    let names: BTreeSet<&str> = secrets_a
        .iter()
        .chain(secrets_b.iter())
        .map(|s| s.name.as_str())
        .collect();

    let names_a: BTreeSet<&str> = secrets_a.iter().map(|s| s.name.as_str()).collect();
    let names_b: BTreeSet<&str> = secrets_b.iter().map(|s| s.name.as_str()).collect();

    // Build diff entries by fetching values for secrets in both envs
    let sp = progress::spinner("Comparing secrets...", progress);
    let mut entries: Vec<DiffEntry> = Vec::new();

    for name in &names {
        let in_a = names_a.contains(name);
        let in_b = names_b.contains(name);

        let (status, value_a, value_b) = match (in_a, in_b) {
            (true, false) => {
                let val = backend.get_secret(name, env_a).await?.value;
                (DiffStatus::OnlyInA, Some(val), None)
            }
            (false, true) => {
                let val = backend.get_secret(name, env_b).await?.value;
                (DiffStatus::OnlyInB, None, Some(val))
            }
            (true, true) => {
                let val_a = backend.get_secret(name, env_a).await?.value;
                let val_b = backend.get_secret(name, env_b).await?.value;
                let status = if val_a == val_b {
                    DiffStatus::Equal
                } else {
                    DiffStatus::Differs
                };
                (status, Some(val_a), Some(val_b))
            }
            (false, false) => unreachable!(),
        };

        entries.push(DiffEntry {
            name: name.to_string(),
            status,
            value_a,
            value_b,
        });
    }
    sp.finish_and_clear();

    // Filter out equal entries unless showing values
    let display_entries: Vec<&DiffEntry> = if show_values {
        entries.iter().collect()
    } else {
        entries
            .iter()
            .filter(|e| e.status != DiffStatus::Equal)
            .collect()
    };

    if display_entries.is_empty() {
        match format {
            OutputFormat::Text => println!("No differences found."),
            OutputFormat::Json => println!("[]"),
        }
        return Ok(());
    }

    match format {
        OutputFormat::Text => print_text(&display_entries, env_a, env_b, show_values),
        OutputFormat::Json => print_json(&display_entries, show_values)?,
    }

    Ok(())
}

/// Format a value for text display.
fn format_value(value: &Option<String>, show_values: bool) -> String {
    match value {
        None => style("(not set)").dim().to_string(),
        Some(v) => {
            if show_values {
                v.clone()
            } else {
                "***".to_string()
            }
        }
    }
}

/// Print diff results as a text table.
fn print_text(entries: &[&DiffEntry], env_a: &str, env_b: &str, show_values: bool) {
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["NAME", env_a, env_b]);

    for entry in entries {
        let name = match entry.status {
            DiffStatus::OnlyInA => style(&entry.name).green().to_string(),
            DiffStatus::OnlyInB => style(&entry.name).red().to_string(),
            DiffStatus::Differs => style(&entry.name).yellow().to_string(),
            DiffStatus::Equal => entry.name.clone(),
        };
        table.add_row(vec![
            name,
            format_value(&entry.value_a, show_values),
            format_value(&entry.value_b, show_values),
        ]);
    }

    println!("{table}");
}

/// Print diff results as JSON.
fn print_json(entries: &[&DiffEntry], show_values: bool) -> Result<(), ZuulError> {
    let json_entries: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            let status = match e.status {
                DiffStatus::Equal => "equal",
                DiffStatus::Differs => "differs",
                DiffStatus::OnlyInA => "only_in_a",
                DiffStatus::OnlyInB => "only_in_b",
            };

            let mut obj = serde_json::json!({
                "name": e.name,
                "status": status,
            });

            if show_values {
                obj["value_a"] = match &e.value_a {
                    Some(v) => serde_json::Value::String(v.clone()),
                    None => serde_json::Value::Null,
                };
                obj["value_b"] = match &e.value_b {
                    Some(v) => serde_json::Value::String(v.clone()),
                    None => serde_json::Value::Null,
                };
            }

            obj
        })
        .collect();

    let json = serde_json::to_string_pretty(&json_entries)
        .map_err(|e| ZuulError::Backend(format!("Failed to serialize: {e}")))?;
    println!("{json}");
    Ok(())
}
