//! Journal tests verifying crash-recoverable batch operations (item 5.11).

use zuul::cli;
use zuul::journal;
use zuul::progress::{BatchContext, ProgressOpts};

use super::common::{AccessLevel, MockBackend};

const PROGRESS: ProgressOpts = ProgressOpts {
    non_interactive: true,
};

fn batch_ctx(dir: &std::path::Path) -> BatchContext {
    BatchContext {
        progress: PROGRESS,
        project_root: Some(dir.to_path_buf()),
    }
}

fn admin_backend() -> MockBackend {
    MockBackend::new(AccessLevel::Admin)
}

// ---------------------------------------------------------------------------
// Journal lock
// ---------------------------------------------------------------------------

#[tokio::test]
async fn journal_lock_prevents_concurrent_import() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("dev", None);

    let env_file = dir.path().join("secrets.env");
    std::fs::write(&env_file, "KEY=value\n").unwrap();

    journal::ensure_zuul_dir(dir.path()).unwrap();
    let fake = journal::Journal::new(
        journal::OperationType::Import,
        serde_json::json!({"environment": "other"}),
        vec![journal::step("set_secret", "X")],
    );
    journal::save_journal(dir.path(), &fake).unwrap();

    let ctx = batch_ctx(dir.path());
    let err = cli::import::run(&backend, "dev", &env_file, None, false, false, &ctx)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("zuul recover"));
    assert!(!backend.has_secret("KEY", "dev"));
}

// ---------------------------------------------------------------------------
// import with journal
// ---------------------------------------------------------------------------

#[tokio::test]
async fn import_creates_and_cleans_journal() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("dev", None);

    let env_file = dir.path().join("secrets.env");
    std::fs::write(&env_file, "DB_URL=postgres://localhost\nAPI_KEY=sk_test\n").unwrap();

    std::fs::write(
        dir.path().join(".zuul.toml"),
        "[backend]\ntype = \"gcp-secret-manager\"\n",
    )
    .unwrap();

    let ctx = batch_ctx(dir.path());
    cli::import::run(&backend, "dev", &env_file, None, false, false, &ctx)
        .await
        .unwrap();

    assert_eq!(
        backend.get_value("DB_URL", "dev").as_deref(),
        Some("postgres://localhost")
    );
    assert_eq!(
        backend.get_value("API_KEY", "dev").as_deref(),
        Some("sk_test")
    );
    assert!(!journal::journal_path(dir.path()).exists());
}

#[tokio::test]
async fn import_dry_run_does_not_create_journal() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("dev", None);

    let env_file = dir.path().join("secrets.env");
    std::fs::write(&env_file, "KEY=value\n").unwrap();

    let ctx = batch_ctx(dir.path());
    cli::import::run(
        &backend, "dev", &env_file, None, false, true, // dry_run
        &ctx,
    )
    .await
    .unwrap();

    assert!(!dir.path().join(".zuul").exists());
    assert!(!backend.has_secret("KEY", "dev"));
}
