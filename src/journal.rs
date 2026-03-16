use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::ZuulError;

/// Directory name for zuul local state.
const ZUUL_DIR: &str = ".zuul";
/// Journal file name within the `.zuul/` directory.
const JOURNAL_FILE: &str = "journal.json";
/// Gitignore file name.
const GITIGNORE_FILE: &str = ".gitignore";
/// Entry to add to `.gitignore`.
const GITIGNORE_ENTRY: &str = ".zuul/";

/// The type of batch operation being journaled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    EnvDelete,
    EnvRename,
    Import,
}

/// Status of an individual journal step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Completed,
}

/// A single step within a journaled batch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalStep {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub status: StepStatus,
}

/// Operation journal for crash-recoverable batch operations.
///
/// Written to `<project-root>/.zuul/journal.json` before a batch operation
/// starts. Each step is flushed to disk after completion. The file is deleted
/// on successful completion and acts as a lock to prevent concurrent batch
/// operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Journal {
    pub operation: OperationType,
    pub started_at: DateTime<Utc>,
    pub params: serde_json::Value,
    pub steps: Vec<JournalStep>,
}

impl Journal {
    /// Create a new journal for a batch operation.
    pub fn new(
        operation: OperationType,
        params: serde_json::Value,
        steps: Vec<JournalStep>,
    ) -> Self {
        Self {
            operation,
            started_at: Utc::now(),
            params,
            steps,
        }
    }

    /// Return the index of the first pending step, if any.
    pub fn first_pending(&self) -> Option<usize> {
        self.steps
            .iter()
            .position(|s| s.status == StepStatus::Pending)
    }

    /// Mark a step as completed by index.
    pub fn mark_completed(&mut self, index: usize) {
        if let Some(step) = self.steps.get_mut(index) {
            step.status = StepStatus::Completed;
        }
    }

    /// Returns `true` if all steps are completed.
    pub fn is_complete(&self) -> bool {
        self.steps.iter().all(|s| s.status == StepStatus::Completed)
    }

    /// Count of completed steps.
    pub fn completed_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| s.status == StepStatus::Completed)
            .count()
    }
}

/// Build a pending step with an action and target.
pub fn step(action: &str, target: &str) -> JournalStep {
    JournalStep {
        action: action.to_string(),
        target: Some(target.to_string()),
        status: StepStatus::Pending,
    }
}

/// Build a pending step with an action but no target.
pub fn step_no_target(action: &str) -> JournalStep {
    JournalStep {
        action: action.to_string(),
        target: None,
        status: StepStatus::Pending,
    }
}

/// Return the path to `<project_root>/.zuul/journal.json`.
pub fn journal_path(project_root: &Path) -> PathBuf {
    project_root.join(ZUUL_DIR).join(JOURNAL_FILE)
}

/// Create the `.zuul/` directory if it does not exist and ensure it is
/// listed in `.gitignore`.
pub fn ensure_zuul_dir(project_root: &Path) -> Result<(), ZuulError> {
    let dir = project_root.join(ZUUL_DIR);
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| ZuulError::Config(format!("Failed to create {ZUUL_DIR}/: {e}")))?;
    }
    add_zuul_dir_to_gitignore(project_root)?;
    Ok(())
}

/// Ensure `.zuul/` is listed in `.gitignore`.
fn add_zuul_dir_to_gitignore(project_root: &Path) -> Result<(), ZuulError> {
    let gitignore_path = project_root.join(GITIGNORE_FILE);

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)
            .map_err(|e| ZuulError::Config(format!("Failed to read {GITIGNORE_FILE}: {e}")))?;

        if content.lines().any(|line| line.trim() == GITIGNORE_ENTRY) {
            return Ok(());
        }

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&gitignore_path)
            .map_err(|e| ZuulError::Config(format!("Failed to open {GITIGNORE_FILE}: {e}")))?;

        if !content.ends_with('\n') && !content.is_empty() {
            writeln!(file)
                .map_err(|e| ZuulError::Config(format!("Failed to write {GITIGNORE_FILE}: {e}")))?;
        }

        writeln!(file, "{GITIGNORE_ENTRY}")
            .map_err(|e| ZuulError::Config(format!("Failed to write {GITIGNORE_FILE}: {e}")))?;
    } else {
        fs::write(&gitignore_path, format!("{GITIGNORE_ENTRY}\n"))
            .map_err(|e| ZuulError::Config(format!("Failed to create {GITIGNORE_FILE}: {e}")))?;
    }

    Ok(())
}

/// Load an existing journal from disk. Returns `None` if no journal exists.
pub fn load_journal(project_root: &Path) -> Result<Option<Journal>, ZuulError> {
    let path = journal_path(project_root);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| ZuulError::Config(format!("Failed to read journal: {e}")))?;
    let journal: Journal = serde_json::from_str(&content)
        .map_err(|e| ZuulError::Config(format!("Failed to parse journal: {e}")))?;
    Ok(Some(journal))
}

/// Write the journal to disk, flushing all data.
pub fn save_journal(project_root: &Path, journal: &Journal) -> Result<(), ZuulError> {
    ensure_zuul_dir(project_root)?;
    let path = journal_path(project_root);
    let content = serde_json::to_string_pretty(journal)
        .map_err(|e| ZuulError::Config(format!("Failed to serialize journal: {e}")))?;
    fs::write(&path, content)
        .map_err(|e| ZuulError::Config(format!("Failed to write journal: {e}")))?;
    Ok(())
}

/// Delete the journal file after a successful batch operation.
pub fn delete_journal(project_root: &Path) -> Result<(), ZuulError> {
    let path = journal_path(project_root);
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|e| ZuulError::Config(format!("Failed to delete journal: {e}")))?;
    }
    Ok(())
}

/// Check whether a journal already exists (acting as a lock).
///
/// Returns an error if a journal is found, directing the user to `zuul recover`.
pub fn check_lock(project_root: &Path) -> Result<(), ZuulError> {
    let path = journal_path(project_root);
    if path.exists() {
        return Err(ZuulError::Config(
            "An incomplete batch operation was found (.zuul/journal.json exists). \
             Run 'zuul recover status' to inspect it, or 'zuul recover abort' to discard it."
                .to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_journal() -> Journal {
        Journal::new(
            OperationType::EnvDelete,
            serde_json::json!({ "environment": "staging" }),
            vec![
                step("delete_secret", "zuul__staging__DB_URL"),
                step("delete_secret", "zuul__staging__API_KEY"),
                step_no_target("update_registry"),
            ],
        )
    }

    #[test]
    fn serialization_roundtrip() {
        let journal = sample_journal();
        let json = serde_json::to_string_pretty(&journal).unwrap();
        let parsed: Journal = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.operation, OperationType::EnvDelete);
        assert_eq!(parsed.steps.len(), 3);
        assert_eq!(parsed.steps[0].action, "delete_secret");
        assert_eq!(
            parsed.steps[0].target.as_deref(),
            Some("zuul__staging__DB_URL")
        );
        assert_eq!(parsed.steps[0].status, StepStatus::Pending);
        assert!(parsed.steps[2].target.is_none());
    }

    #[test]
    fn step_progression() {
        let mut journal = sample_journal();

        assert_eq!(journal.first_pending(), Some(0));
        assert_eq!(journal.completed_count(), 0);
        assert!(!journal.is_complete());

        journal.mark_completed(0);
        assert_eq!(journal.first_pending(), Some(1));
        assert_eq!(journal.completed_count(), 1);

        journal.mark_completed(1);
        assert_eq!(journal.first_pending(), Some(2));
        assert_eq!(journal.completed_count(), 2);

        journal.mark_completed(2);
        assert_eq!(journal.first_pending(), None);
        assert_eq!(journal.completed_count(), 3);
        assert!(journal.is_complete());
    }

    #[test]
    fn file_operations() {
        let dir = tempfile::tempdir().unwrap();
        let journal = sample_journal();

        // No journal initially
        assert!(load_journal(dir.path()).unwrap().is_none());

        // Save and load
        save_journal(dir.path(), &journal).unwrap();
        let loaded = load_journal(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.operation, OperationType::EnvDelete);
        assert_eq!(loaded.steps.len(), 3);

        // .zuul/ directory was created
        assert!(dir.path().join(ZUUL_DIR).is_dir());

        // Lock check detects journal
        let result = check_lock(dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("zuul recover"));

        // Delete journal
        delete_journal(dir.path()).unwrap();
        assert!(load_journal(dir.path()).unwrap().is_none());

        // Lock check passes after deletion
        check_lock(dir.path()).unwrap();
    }

    #[test]
    fn gitignore_created_on_first_use() {
        let dir = tempfile::tempdir().unwrap();
        ensure_zuul_dir(dir.path()).unwrap();

        let content = fs::read_to_string(dir.path().join(GITIGNORE_FILE)).unwrap();
        assert!(content.contains(GITIGNORE_ENTRY));
    }

    #[test]
    fn gitignore_appended_if_missing_entry() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(GITIGNORE_FILE), "node_modules/\n").unwrap();

        ensure_zuul_dir(dir.path()).unwrap();

        let content = fs::read_to_string(dir.path().join(GITIGNORE_FILE)).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains(GITIGNORE_ENTRY));
    }

    #[test]
    fn gitignore_not_duplicated() {
        let dir = tempfile::tempdir().unwrap();
        let initial = format!("node_modules/\n{GITIGNORE_ENTRY}\n");
        fs::write(dir.path().join(GITIGNORE_FILE), &initial).unwrap();

        ensure_zuul_dir(dir.path()).unwrap();

        let content = fs::read_to_string(dir.path().join(GITIGNORE_FILE)).unwrap();
        assert_eq!(content, initial);
    }

    #[test]
    fn gitignore_newline_appended_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(GITIGNORE_FILE), "node_modules/").unwrap();

        ensure_zuul_dir(dir.path()).unwrap();

        let content = fs::read_to_string(dir.path().join(GITIGNORE_FILE)).unwrap();
        assert!(content.contains("node_modules/\n"));
        assert!(content.contains(GITIGNORE_ENTRY));
    }

    #[test]
    fn rename_journal_serialization() {
        let journal = Journal::new(
            OperationType::EnvRename,
            serde_json::json!({
                "old_name": "staging",
                "new_name": "staging-v2"
            }),
            vec![
                step("rename_secret", "DB_URL"),
                step("rename_secret", "API_KEY"),
                step_no_target("update_registry"),
            ],
        );

        let json = serde_json::to_string_pretty(&journal).unwrap();
        let parsed: Journal = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.operation, OperationType::EnvRename);
        assert_eq!(parsed.params["old_name"], "staging");
        assert_eq!(parsed.params["new_name"], "staging-v2");
    }

    #[test]
    fn import_journal_serialization() {
        let journal = Journal::new(
            OperationType::Import,
            serde_json::json!({
                "environment": "dev",
                "file": "secrets.env"
            }),
            vec![step("set_secret", "DB_URL"), step("set_secret", "API_KEY")],
        );

        let json = serde_json::to_string_pretty(&journal).unwrap();
        let parsed: Journal = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.operation, OperationType::Import);
        assert_eq!(parsed.steps.len(), 2);
    }

    #[test]
    fn save_and_update_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut journal = sample_journal();

        save_journal(dir.path(), &journal).unwrap();

        // Simulate step completion with flush
        journal.mark_completed(0);
        save_journal(dir.path(), &journal).unwrap();

        let loaded = load_journal(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.steps[0].status, StepStatus::Completed);
        assert_eq!(loaded.steps[1].status, StepStatus::Pending);
    }
}
