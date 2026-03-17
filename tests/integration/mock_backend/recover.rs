//! Tests for `zuul recover` subcommands.

use zuul::cli::recover;
use zuul::journal;
use zuul::progress::ProgressOpts;

use super::common::{AccessLevel, MockBackend};

const PROGRESS: ProgressOpts = ProgressOpts {
    non_interactive: true,
};

fn admin_backend() -> MockBackend {
    MockBackend::new(AccessLevel::Admin)
}

// ---------------------------------------------------------------------------
// recover status
// ---------------------------------------------------------------------------

#[test]
fn status_no_journal_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    // No journal file — should print "No incomplete operations" and succeed.
    recover::status(dir.path()).unwrap();
}

#[test]
fn status_with_journal_shows_progress() {
    let dir = tempfile::tempdir().unwrap();

    let mut jrnl = journal::Journal::new(
        journal::OperationType::EnvClear,
        serde_json::json!({ "environment": "staging" }),
        vec![
            journal::step("delete_secret", "DB_URL"),
            journal::step("delete_secret", "API_KEY"),
            journal::step("delete_secret", "REDIS_URL"),
        ],
    );
    // Simulate 1 of 3 completed.
    jrnl.mark_completed(0);
    journal::save_journal(dir.path(), &jrnl).unwrap();

    // Should succeed without error (output goes to stdout).
    recover::status(dir.path()).unwrap();

    // Journal should still exist (status is read-only).
    assert!(journal::journal_path(dir.path()).exists());
}

// ---------------------------------------------------------------------------
// recover abort
// ---------------------------------------------------------------------------

#[test]
fn abort_no_journal_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    recover::abort(dir.path(), true, true).unwrap();
}

#[test]
fn abort_deletes_journal() {
    let dir = tempfile::tempdir().unwrap();

    let jrnl = journal::Journal::new(
        journal::OperationType::Import,
        serde_json::json!({ "environment": "dev", "file": "secrets.env" }),
        vec![
            journal::step("set_secret", "DB_URL"),
            journal::step("set_secret", "API_KEY"),
        ],
    );
    journal::save_journal(dir.path(), &jrnl).unwrap();

    recover::abort(dir.path(), true, true).unwrap();

    assert!(!journal::journal_path(dir.path()).exists());
}

// ---------------------------------------------------------------------------
// recover resume — env clear
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resume_env_clear_completes_pending_steps() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("staging", None);
    backend.seed_secret("DB_URL", "staging", "postgres://staging");
    backend.seed_secret("API_KEY", "staging", "sk_staging");
    backend.seed_secret("REDIS_URL", "staging", "redis://staging");

    // Simulate an interrupted clear: first step completed, two pending.
    let mut jrnl = journal::Journal::new(
        journal::OperationType::EnvClear,
        serde_json::json!({ "environment": "staging" }),
        vec![
            journal::step("delete_secret", "DB_URL"),
            journal::step("delete_secret", "API_KEY"),
            journal::step("delete_secret", "REDIS_URL"),
        ],
    );
    jrnl.mark_completed(0);
    journal::save_journal(dir.path(), &jrnl).unwrap();

    // Manually remove DB_URL to simulate the completed step's effect.
    backend.remove_secret("DB_URL", "staging");

    recover::resume(&backend, dir.path(), true, true, PROGRESS)
        .await
        .unwrap();

    assert!(!backend.has_secret("API_KEY", "staging"));
    assert!(!backend.has_secret("REDIS_URL", "staging"));
    assert!(!journal::journal_path(dir.path()).exists());
}

// ---------------------------------------------------------------------------
// recover resume — env copy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resume_env_copy_completes_pending_steps() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("src", None);
    backend.seed_environment("dst", None);
    backend.seed_secret("DB_URL", "src", "postgres://src");
    backend.seed_secret("API_KEY", "src", "sk_src");

    // Simulate an interrupted copy: DB_URL already copied, API_KEY pending.
    let mut jrnl = journal::Journal::new(
        journal::OperationType::EnvCopy,
        serde_json::json!({ "from": "src", "to": "dst" }),
        vec![
            journal::step("set_secret", "DB_URL"),
            journal::step("set_secret", "API_KEY"),
        ],
    );
    jrnl.mark_completed(0);
    journal::save_journal(dir.path(), &jrnl).unwrap();

    // DB_URL already in dst from the completed step.
    backend.seed_secret("DB_URL", "dst", "postgres://src");

    recover::resume(&backend, dir.path(), true, true, PROGRESS)
        .await
        .unwrap();

    assert_eq!(
        backend.get_value("API_KEY", "dst").as_deref(),
        Some("sk_src")
    );
    assert!(!journal::journal_path(dir.path()).exists());
}

// ---------------------------------------------------------------------------
// recover resume — import
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resume_import_completes_pending_steps() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("dev", None);

    // Write the source file that the journal references.
    let env_file = dir.path().join("secrets.env");
    std::fs::write(&env_file, "DB_URL=postgres://dev\nAPI_KEY=sk_dev\n").unwrap();

    // Simulate an interrupted import: DB_URL done, API_KEY pending.
    let mut jrnl = journal::Journal::new(
        journal::OperationType::Import,
        serde_json::json!({
            "environment": "dev",
            "file": env_file.display().to_string(),
        }),
        vec![
            journal::step("set_secret", "DB_URL"),
            journal::step("set_secret", "API_KEY"),
        ],
    );
    jrnl.mark_completed(0);
    journal::save_journal(dir.path(), &jrnl).unwrap();

    backend.seed_secret("DB_URL", "dev", "postgres://dev");

    recover::resume(&backend, dir.path(), true, true, PROGRESS)
        .await
        .unwrap();

    assert_eq!(
        backend.get_value("API_KEY", "dev").as_deref(),
        Some("sk_dev")
    );
    assert!(!journal::journal_path(dir.path()).exists());
}

// ---------------------------------------------------------------------------
// recover resume — metadata set
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resume_metadata_set_completes_pending_steps() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("dev", None);
    backend.seed_environment("staging", None);
    backend.seed_secret("KEY", "dev", "d");
    backend.seed_secret("KEY", "staging", "s");

    // Simulate interrupted metadata set: dev done, staging pending.
    let mut jrnl = journal::Journal::new(
        journal::OperationType::MetadataSet,
        serde_json::json!({
            "secret": "KEY",
            "key": "owner",
            "value": "ops-team",
        }),
        vec![
            journal::step("set_metadata", "dev"),
            journal::step("set_metadata", "staging"),
        ],
    );
    jrnl.mark_completed(0);
    journal::save_journal(dir.path(), &jrnl).unwrap();

    backend.seed_metadata("KEY", "dev", "owner", "ops-team");

    recover::resume(&backend, dir.path(), true, true, PROGRESS)
        .await
        .unwrap();

    let meta = backend.get_meta("KEY", "staging");
    assert_eq!(meta.get("owner").map(String::as_str), Some("ops-team"));
    assert!(!journal::journal_path(dir.path()).exists());
}

// ---------------------------------------------------------------------------
// recover resume — metadata delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resume_metadata_delete_completes_pending_steps() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("dev", None);
    backend.seed_environment("staging", None);
    backend.seed_secret("KEY", "dev", "d");
    backend.seed_secret("KEY", "staging", "s");
    backend.seed_metadata("KEY", "staging", "tag", "v1");

    // Simulate interrupted metadata delete: dev done, staging pending.
    let mut jrnl = journal::Journal::new(
        journal::OperationType::MetadataDelete,
        serde_json::json!({
            "secret": "KEY",
            "key": "tag",
        }),
        vec![
            journal::step("delete_metadata", "dev"),
            journal::step("delete_metadata", "staging"),
        ],
    );
    jrnl.mark_completed(0);
    journal::save_journal(dir.path(), &jrnl).unwrap();

    recover::resume(&backend, dir.path(), true, true, PROGRESS)
        .await
        .unwrap();

    let meta = backend.get_meta("KEY", "staging");
    assert!(!meta.contains_key("tag"));
    assert!(!journal::journal_path(dir.path()).exists());
}

// ---------------------------------------------------------------------------
// recover resume — no journal
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resume_no_journal_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();

    recover::resume(&backend, dir.path(), true, true, PROGRESS)
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// recover resume — already complete journal
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resume_already_complete_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();

    let mut jrnl = journal::Journal::new(
        journal::OperationType::EnvClear,
        serde_json::json!({ "environment": "staging" }),
        vec![journal::step("delete_secret", "KEY")],
    );
    jrnl.mark_completed(0);
    journal::save_journal(dir.path(), &jrnl).unwrap();

    recover::resume(&backend, dir.path(), true, true, PROGRESS)
        .await
        .unwrap();

    assert!(!journal::journal_path(dir.path()).exists());
}
