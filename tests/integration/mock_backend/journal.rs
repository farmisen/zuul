//! Journal tests verifying crash-recoverable batch operations.

use zuul::backend::Backend;
use zuul::cli::{self, OutputFormat};
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

#[tokio::test]
async fn journal_lock_prevents_concurrent_clear() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("staging", None);
    backend.seed_secret("KEY", "staging", "val");

    journal::ensure_zuul_dir(dir.path()).unwrap();
    let fake = journal::Journal::new(
        journal::OperationType::Import,
        serde_json::json!({"environment": "other"}),
        vec![journal::step("set_secret", "X")],
    );
    journal::save_journal(dir.path(), &fake).unwrap();

    let ctx = batch_ctx(dir.path());
    let err = cli::env::clear(&backend, "staging", true, false, &OutputFormat::Text, &ctx)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("zuul recover"));
    assert!(backend.has_secret("KEY", "staging"));
}

#[tokio::test]
async fn journal_lock_prevents_concurrent_copy() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("src", None);
    backend.seed_environment("dst", None);
    backend.seed_secret("KEY", "src", "val");

    journal::ensure_zuul_dir(dir.path()).unwrap();
    let fake = journal::Journal::new(
        journal::OperationType::Import,
        serde_json::json!({"environment": "other"}),
        vec![journal::step("set_secret", "X")],
    );
    journal::save_journal(dir.path(), &fake).unwrap();

    let ctx = batch_ctx(dir.path());
    let err = cli::env::copy(
        &backend,
        "src",
        "dst",
        true,
        false,
        &OutputFormat::Text,
        &ctx,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("zuul recover"));
    assert!(!backend.has_secret("KEY", "dst"));
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
        "[backend]\ntype = \"mock\"\n",
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

// ---------------------------------------------------------------------------
// env clear with journal
// ---------------------------------------------------------------------------

#[tokio::test]
async fn clear_creates_and_cleans_journal() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("staging", None);
    backend.seed_secret("DB_URL", "staging", "postgres://localhost");
    backend.seed_secret("API_KEY", "staging", "sk_test_123");

    let ctx = batch_ctx(dir.path());
    cli::env::clear(&backend, "staging", true, false, &OutputFormat::Text, &ctx)
        .await
        .unwrap();

    assert!(!journal::journal_path(dir.path()).exists());
    assert!(dir.path().join(".zuul").is_dir());
    assert!(!backend.has_secret("DB_URL", "staging"));
    assert!(!backend.has_secret("API_KEY", "staging"));
    // Environment still exists after clear.
    backend.get_environment("staging").await.unwrap();
}

#[tokio::test]
async fn clear_without_secrets_skips_journal() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("empty", None);

    let ctx = batch_ctx(dir.path());
    cli::env::clear(&backend, "empty", true, false, &OutputFormat::Text, &ctx)
        .await
        .unwrap();

    assert!(!dir.path().join(".zuul").exists());
}

// ---------------------------------------------------------------------------
// env copy with journal
// ---------------------------------------------------------------------------

#[tokio::test]
async fn copy_creates_and_cleans_journal() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("src", None);
    backend.seed_environment("dst", None);
    backend.seed_secret("DB_URL", "src", "postgres://src");
    backend.seed_secret("API_KEY", "src", "sk_src");

    let ctx = batch_ctx(dir.path());
    cli::env::copy(
        &backend,
        "src",
        "dst",
        true,
        false,
        &OutputFormat::Text,
        &ctx,
    )
    .await
    .unwrap();

    assert!(!journal::journal_path(dir.path()).exists());
    assert_eq!(
        backend.get_value("DB_URL", "dst").as_deref(),
        Some("postgres://src")
    );
    assert_eq!(
        backend.get_value("API_KEY", "dst").as_deref(),
        Some("sk_src")
    );
}

#[tokio::test]
async fn copy_dry_run_does_not_create_journal() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("src", None);
    backend.seed_environment("dst", None);
    backend.seed_secret("KEY", "src", "val");

    let ctx = batch_ctx(dir.path());
    cli::env::copy(
        &backend,
        "src",
        "dst",
        true,
        true,
        &OutputFormat::Text,
        &ctx,
    )
    .await
    .unwrap();

    assert!(!dir.path().join(".zuul").exists());
    assert!(!backend.has_secret("KEY", "dst"));
}
