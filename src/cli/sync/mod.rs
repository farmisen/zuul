pub mod fly;
pub mod netlify;

use std::collections::HashMap;

use console::style;

use crate::error::ZuulError;

/// Trait that all sync platform targets must implement.
///
/// Each platform wraps its CLI tool (e.g., `netlify`, `vercel`, `fly`)
/// and implements list/set/delete operations via subprocess calls.
pub trait SyncTarget {
    /// Platform display name (e.g., "Netlify").
    fn name(&self) -> &str;

    /// Target description for summary output (e.g., "Netlify/production").
    fn target_description(&self) -> String;

    /// Fetch current environment variables from the platform.
    fn list_vars(&self) -> Result<HashMap<String, String>, ZuulError>;

    /// Set an environment variable on the platform.
    fn set_var(&self, name: &str, value: &str) -> Result<(), ZuulError>;

    /// Remove an environment variable from the platform.
    fn unset_var(&self, name: &str) -> Result<(), ZuulError>;
}

/// A single change to be applied during sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    /// Secret exists in zuul but not on the platform.
    Create { name: String, value: String },
    /// Secret exists on both but values differ.
    Update { name: String, value: String },
    /// Secret exists on the platform but not in zuul (only with --prune).
    Prune { name: String },
    /// Secret exists on both with the same value.
    Unchanged { name: String },
}

/// Summary of a sync operation.
#[derive(Debug, Default)]
pub struct SyncSummary {
    pub created: usize,
    pub updated: usize,
    pub pruned: usize,
    pub unchanged: usize,
}

/// Options for executing a sync operation.
pub struct SyncOpts<'a> {
    pub target: &'a dyn SyncTarget,
    pub actions: &'a [SyncAction],
    pub dry_run: bool,
    pub prune: bool,
    pub force: bool,
    pub non_interactive: bool,
}

/// Compute the diff between zuul secrets and platform variables.
///
/// Returns a list of actions to take, sorted by name.
pub fn compute_diff(
    zuul_secrets: &HashMap<String, String>,
    platform_vars: &HashMap<String, String>,
    prune: bool,
) -> Vec<SyncAction> {
    let mut actions = Vec::new();

    // Check zuul secrets against platform
    for (name, value) in zuul_secrets {
        match platform_vars.get(name) {
            None => actions.push(SyncAction::Create {
                name: name.clone(),
                value: value.clone(),
            }),
            Some(existing) if existing != value => actions.push(SyncAction::Update {
                name: name.clone(),
                value: value.clone(),
            }),
            Some(_) => actions.push(SyncAction::Unchanged { name: name.clone() }),
        }
    }

    // Check for platform vars not in zuul (prune candidates)
    for name in platform_vars.keys() {
        if !zuul_secrets.contains_key(name) {
            if prune {
                actions.push(SyncAction::Prune { name: name.clone() });
            } else {
                actions.push(SyncAction::Unchanged { name: name.clone() });
            }
        }
    }

    actions.sort_by(|a, b| action_name(a).cmp(action_name(b)));
    actions
}

/// Extract the name from a SyncAction.
fn action_name(action: &SyncAction) -> &str {
    match action {
        SyncAction::Create { name, .. }
        | SyncAction::Update { name, .. }
        | SyncAction::Prune { name }
        | SyncAction::Unchanged { name } => name,
    }
}

/// Print the dry-run preview of sync actions.
fn print_dry_run(actions: &[SyncAction], target_desc: &str) {
    println!("Dry run: changes that would be synced to {target_desc}:\n");

    let mut has_changes = false;
    for action in actions {
        match action {
            SyncAction::Create { name, .. } => {
                has_changes = true;
                println!("  {} {name}", style("+").green());
            }
            SyncAction::Update { name, .. } => {
                has_changes = true;
                println!("  {} {name}", style("~").yellow());
            }
            SyncAction::Prune { name } => {
                has_changes = true;
                println!("  {} {name}", style("-").red());
            }
            SyncAction::Unchanged { .. } => {}
        }
    }

    if !has_changes {
        println!("  (no changes)");
    }
    println!();

    let summary = summarize(actions);
    print_summary_line(&summary, target_desc);
}

/// Print the post-sync summary line.
fn print_summary_line(summary: &SyncSummary, target_desc: &str) {
    let total = summary.created + summary.updated + summary.pruned + summary.unchanged;
    let mut parts = Vec::new();
    if summary.created > 0 {
        parts.push(format!("{} created", summary.created));
    }
    if summary.updated > 0 {
        parts.push(format!("{} updated", summary.updated));
    }
    if summary.pruned > 0 {
        parts.push(format!("{} pruned", summary.pruned));
    }
    if summary.unchanged > 0 {
        parts.push(format!("{} unchanged", summary.unchanged));
    }

    println!(
        "Synced {total} secrets to {target_desc} ({})",
        parts.join(", ")
    );
}

/// Count actions into a summary.
pub fn summarize(actions: &[SyncAction]) -> SyncSummary {
    let mut s = SyncSummary::default();
    for action in actions {
        match action {
            SyncAction::Create { .. } => s.created += 1,
            SyncAction::Update { .. } => s.updated += 1,
            SyncAction::Prune { .. } => s.pruned += 1,
            SyncAction::Unchanged { .. } => s.unchanged += 1,
        }
    }
    s
}

/// Execute a sync: apply actions to the platform via the SyncTarget trait.
pub fn execute_sync(opts: &SyncOpts<'_>) -> Result<SyncSummary, ZuulError> {
    let target_desc = opts.target.target_description();

    if opts.dry_run {
        print_dry_run(opts.actions, &target_desc);
        return Ok(summarize(opts.actions));
    }

    // Check if there are prune actions that need confirmation
    let prune_count = opts
        .actions
        .iter()
        .filter(|a| matches!(a, SyncAction::Prune { .. }))
        .count();
    if opts.prune && prune_count > 0 {
        let msg = format!("{prune_count} secret(s) will be removed from {target_desc}. Continue?");
        if !crate::prompt::confirm(&msg, opts.force, opts.non_interactive)? {
            return Err(ZuulError::Validation("Sync cancelled.".to_string()));
        }
    }

    // Apply actions via the trait
    for action in opts.actions {
        match action {
            SyncAction::Create { name, value } | SyncAction::Update { name, value } => {
                opts.target.set_var(name, value)?;
            }
            SyncAction::Prune { name } => {
                opts.target.unset_var(name)?;
            }
            SyncAction::Unchanged { .. } => {}
        }
    }

    let summary = summarize(opts.actions);
    print_summary_line(&summary, &target_desc);
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn diff_all_new() {
        let zuul = secrets(&[("A", "1"), ("B", "2")]);
        let platform = HashMap::new();
        let actions = compute_diff(&zuul, &platform, false);
        assert_eq!(actions.len(), 2);
        assert!(
            actions
                .iter()
                .all(|a| matches!(a, SyncAction::Create { .. }))
        );
    }

    #[test]
    fn diff_all_unchanged() {
        let zuul = secrets(&[("A", "1"), ("B", "2")]);
        let platform = secrets(&[("A", "1"), ("B", "2")]);
        let actions = compute_diff(&zuul, &platform, false);
        assert_eq!(actions.len(), 2);
        assert!(
            actions
                .iter()
                .all(|a| matches!(a, SyncAction::Unchanged { .. }))
        );
    }

    #[test]
    fn diff_mixed_changes() {
        let zuul = secrets(&[("A", "1"), ("B", "new"), ("C", "3")]);
        let platform = secrets(&[("A", "1"), ("B", "old"), ("D", "extra")]);
        let actions = compute_diff(&zuul, &platform, false);
        assert_eq!(actions.len(), 4);

        let create: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, SyncAction::Create { .. }))
            .collect();
        let update: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, SyncAction::Update { .. }))
            .collect();
        let unchanged: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, SyncAction::Unchanged { .. }))
            .collect();

        assert_eq!(create.len(), 1); // C
        assert_eq!(update.len(), 1); // B
        assert_eq!(unchanged.len(), 2); // A + D (not pruned)
    }

    #[test]
    fn diff_prune_enabled() {
        let zuul = secrets(&[("A", "1")]);
        let platform = secrets(&[("A", "1"), ("EXTRA", "gone")]);
        let actions = compute_diff(&zuul, &platform, true);
        assert_eq!(actions.len(), 2);

        let pruned: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, SyncAction::Prune { .. }))
            .collect();
        assert_eq!(pruned.len(), 1);
    }

    #[test]
    fn diff_prune_disabled_treats_extra_as_unchanged() {
        let zuul = secrets(&[("A", "1")]);
        let platform = secrets(&[("A", "1"), ("EXTRA", "kept")]);
        let actions = compute_diff(&zuul, &platform, false);

        let pruned: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, SyncAction::Prune { .. }))
            .collect();
        assert_eq!(pruned.len(), 0);
    }

    #[test]
    fn diff_empty_both() {
        let actions = compute_diff(&HashMap::new(), &HashMap::new(), false);
        assert!(actions.is_empty());
    }

    #[test]
    fn summary_counts() {
        let actions = vec![
            SyncAction::Create {
                name: "A".into(),
                value: "1".into(),
            },
            SyncAction::Create {
                name: "B".into(),
                value: "2".into(),
            },
            SyncAction::Update {
                name: "C".into(),
                value: "3".into(),
            },
            SyncAction::Prune { name: "D".into() },
            SyncAction::Unchanged { name: "E".into() },
        ];
        let s = summarize(&actions);
        assert_eq!(s.created, 2);
        assert_eq!(s.updated, 1);
        assert_eq!(s.pruned, 1);
        assert_eq!(s.unchanged, 1);
    }
}
