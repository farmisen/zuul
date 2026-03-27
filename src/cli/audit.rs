use std::collections::{BTreeMap, BTreeSet};

use comfy_table::{ContentArrangement, Table};

use crate::backend::Backend;
use crate::cli::OutputFormat;
use crate::error::ZuulError;
use crate::models::AccessBinding;

/// IAM identity prefix for user accounts.
const USER_PREFIX: &str = "user:";
/// IAM identity prefix for service accounts.
const SERVICE_ACCOUNT_PREFIX: &str = "serviceAccount:";

/// Run the `zuul audit` command.
pub async fn run(
    backend: &impl Backend,
    env_filter: Option<&str>,
    identity_filter: Option<&str>,
    format: &OutputFormat,
) -> Result<(), ZuulError> {
    let bindings = backend.audit_access().await?;

    // Fetch known environments so project-wide roles show across all envs
    let known_envs: Vec<String> = backend
        .list_environments()
        .await
        .map(|envs| envs.into_iter().map(|e| e.name).collect())
        .unwrap_or_default();

    let filtered: Vec<_> = bindings
        .into_iter()
        .filter(|b| {
            env_filter
                .is_none_or(|e| b.environment.as_deref() == Some(e) || b.environment.is_none())
        })
        .filter(|b| identity_filter.is_none_or(|i| b.identity == i))
        .collect();

    match format {
        OutputFormat::Json => print_json(&filtered),
        OutputFormat::Text => print_matrix(&filtered, &known_envs),
    }

    Ok(())
}

/// Print bindings as JSON.
fn print_json(bindings: &[AccessBinding]) {
    let json: Vec<_> = bindings
        .iter()
        .map(|b| {
            serde_json::json!({
                "identity": b.identity,
                "environment": b.environment,
                "role": b.role,
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json).unwrap_or_default()
    );
}

/// Print an identity × environment access matrix.
///
/// Merges user identities with their matching developer SA into a single row.
fn print_matrix(bindings: &[AccessBinding], known_envs: &[String]) {
    if bindings.is_empty() {
        println!("No access bindings found.");
        return;
    }

    // Collect unique environments (from bindings + known envs)
    let mut environments = BTreeSet::new();
    for env in known_envs {
        environments.insert(env.clone());
    }
    for b in bindings {
        if let Some(ref env) = b.environment {
            environments.insert(env.clone());
        }
    }

    // Build access map: identity → env → role, and project-wide map
    let mut access: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut project_wide: BTreeMap<String, String> = BTreeMap::new();

    for b in bindings {
        match &b.environment {
            Some(env) => {
                access
                    .entry(b.identity.clone())
                    .or_default()
                    .insert(env.clone(), b.role.clone());
            }
            None => {
                project_wide.insert(b.identity.clone(), b.role.clone());
            }
        }
    }

    // Match user identities with their developer SAs
    let users: Vec<_> = bindings
        .iter()
        .map(|b| &b.identity)
        .filter(|id| id.starts_with(USER_PREFIX))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .cloned()
        .collect();

    let sas: BTreeSet<_> = bindings
        .iter()
        .map(|b| &b.identity)
        .filter(|id| id.starts_with(SERVICE_ACCOUNT_PREFIX))
        .cloned()
        .collect();

    // Build user → SA mapping by matching access patterns
    let mut user_to_sa: BTreeMap<String, String> = BTreeMap::new();
    let mut matched_sas: BTreeSet<String> = BTreeSet::new();

    for user in &users {
        let user_role = get_access_summary(user, &project_wide, &access);
        for sa in &sas {
            if matched_sas.contains(sa) {
                continue;
            }
            let sa_role = get_access_summary(sa, &project_wide, &access);
            if user_role == sa_role {
                user_to_sa.insert(user.clone(), sa.clone());
                matched_sas.insert(sa.clone());
                break;
            }
        }
    }

    // Unmatched SAs (e.g., CI/CD service accounts)
    let unmatched_sas: Vec<_> = sas.difference(&matched_sas).cloned().collect();

    let envs: Vec<_> = environments.into_iter().collect();
    let has_sas = !user_to_sa.is_empty() || !unmatched_sas.is_empty();

    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);

    if envs.is_empty() {
        let mut header = vec!["Identity".to_string()];
        if has_sas {
            header.push("SA".to_string());
        }
        header.push("Role".to_string());
        table.set_header(header);

        for user in &users {
            let role = project_wide.get(user).map(|r| r.as_str()).unwrap_or("-");
            let mut row = vec![user.clone()];
            if has_sas {
                row.push(
                    user_to_sa
                        .get(user)
                        .map(|s| short_sa(s))
                        .unwrap_or_else(|| "-".to_string()),
                );
            }
            row.push(role.to_string());
            table.add_row(row);
        }
        for sa in &unmatched_sas {
            let role = project_wide.get(sa).map(|r| r.as_str()).unwrap_or("-");
            let mut row = vec![sa.clone()];
            if has_sas {
                row.push("-".to_string());
            }
            row.push(role.to_string());
            table.add_row(row);
        }
    } else {
        let mut header = vec!["Identity".to_string()];
        if has_sas {
            header.push("SA".to_string());
        }
        header.extend(envs.iter().cloned());
        table.set_header(header);

        for user in &users {
            let mut row = vec![user.clone()];
            if has_sas {
                row.push(
                    user_to_sa
                        .get(user)
                        .map(|s| short_sa(s))
                        .unwrap_or_else(|| "-".to_string()),
                );
            }
            append_env_cells(&mut row, user, &envs, &project_wide, &access);
            table.add_row(row);
        }
        for sa in &unmatched_sas {
            let mut row = vec![sa.clone()];
            if has_sas {
                row.push("-".to_string());
            }
            append_env_cells(&mut row, sa, &envs, &project_wide, &access);
            table.add_row(row);
        }
    }

    println!("{table}");
}

/// Format a service account identity for display (strip the "serviceAccount:" prefix).
fn short_sa(sa: &str) -> String {
    sa.strip_prefix(SERVICE_ACCOUNT_PREFIX)
        .unwrap_or(sa)
        .to_string()
}

/// Get a comparable access summary string for matching users to SAs.
fn get_access_summary(
    identity: &str,
    project_wide: &BTreeMap<String, String>,
    access: &BTreeMap<String, BTreeMap<String, String>>,
) -> String {
    if let Some(role) = project_wide.get(identity) {
        return format!("*:{role}");
    }
    if let Some(env_map) = access.get(identity) {
        let mut entries: Vec<_> = env_map.iter().map(|(e, r)| format!("{e}:{r}")).collect();
        entries.sort();
        return entries.join(",");
    }
    String::new()
}

/// Append environment role cells to a row.
fn append_env_cells(
    row: &mut Vec<String>,
    identity: &str,
    envs: &[String],
    project_wide: &BTreeMap<String, String>,
    access: &BTreeMap<String, BTreeMap<String, String>>,
) {
    if let Some(role) = project_wide.get(identity) {
        for _ in envs {
            row.push(role.clone());
        }
    } else {
        let env_access = access.get(identity);
        for env in envs {
            let cell = env_access
                .and_then(|a| a.get(env))
                .map(|r| r.as_str())
                .unwrap_or("-");
            row.push(cell.to_string());
        }
    }
}
