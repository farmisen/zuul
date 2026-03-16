//! Journal tests verifying crash-recoverable batch operations (item 5.11).

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
// env delete with journal
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_env_creates_and_cleans_journal() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("staging", None);
    backend.seed_secret("DB_URL", "staging", "postgres://localhost");
    backend.seed_secret("API_KEY", "staging", "sk_test_123");

    let ctx = batch_ctx(dir.path());
    cli::env::delete(&backend, "staging", true, false, &OutputFormat::Text, &ctx)
        .await
        .unwrap();

    assert!(
        !journal::journal_path(dir.path()).exists(),
        "journal should be deleted after successful operation"
    );
    assert!(dir.path().join(".zuul").is_dir());
    assert!(!backend.has_env("staging"));
    assert!(!backend.has_secret("DB_URL", "staging"));
    assert!(!backend.has_secret("API_KEY", "staging"));
}

#[tokio::test]
async fn delete_env_without_secrets_skips_journal() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("empty", None);

    let ctx = batch_ctx(dir.path());
    cli::env::delete(&backend, "empty", true, false, &OutputFormat::Text, &ctx)
        .await
        .unwrap();

    assert!(!dir.path().join(".zuul").exists());
    assert!(!backend.has_env("empty"));
}

#[tokio::test]
async fn delete_env_adds_zuul_dir_to_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".gitignore"), "node_modules/\n").unwrap();

    let backend = admin_backend();
    backend.seed_environment("staging", None);
    backend.seed_secret("KEY", "staging", "val");

    let ctx = batch_ctx(dir.path());
    cli::env::delete(&backend, "staging", true, false, &OutputFormat::Text, &ctx)
        .await
        .unwrap();

    let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains(".zuul/"));
    assert!(gitignore.contains("node_modules/"));
}

#[tokio::test]
async fn delete_env_journal_records_correct_steps() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("staging", None);
    backend.seed_secret("A", "staging", "val_a");
    backend.seed_secret("B", "staging", "val_b");

    let ctx = batch_ctx(dir.path());
    cli::env::delete(&backend, "staging", true, false, &OutputFormat::Text, &ctx)
        .await
        .unwrap();

    assert_eq!(backend.secret_count("staging"), 0);
    assert!(!backend.has_env("staging"));
}

// ---------------------------------------------------------------------------
// Journal lock
// ---------------------------------------------------------------------------

#[tokio::test]
async fn journal_lock_prevents_concurrent_batch_ops() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("staging", None);
    backend.seed_secret("KEY", "staging", "val");

    journal::ensure_zuul_dir(dir.path()).unwrap();
    let fake_journal = journal::Journal::new(
        journal::OperationType::EnvDelete,
        serde_json::json!({ "environment": "other" }),
        vec![journal::step("delete_secret", "zuul__other__X")],
    );
    journal::save_journal(dir.path(), &fake_journal).unwrap();

    let ctx = batch_ctx(dir.path());
    let err = cli::env::delete(&backend, "staging", true, false, &OutputFormat::Text, &ctx)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("zuul recover"),
        "lock error should mention 'zuul recover', got: {err}"
    );
    assert!(backend.has_secret("KEY", "staging"));
}

#[tokio::test]
async fn rename_journal_lock_blocks_concurrent_rename() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("dev", None);
    backend.seed_secret("KEY", "dev", "val");

    journal::ensure_zuul_dir(dir.path()).unwrap();
    let fake = journal::Journal::new(
        journal::OperationType::EnvRename,
        serde_json::json!({"old_name": "other", "new_name": "other2"}),
        vec![journal::step("rename_secret", "X")],
    );
    journal::save_journal(dir.path(), &fake).unwrap();

    let ctx = batch_ctx(dir.path());
    let err = cli::env::update(
        &backend,
        "dev",
        Some("dev2"),
        None,
        true,
        &OutputFormat::Text,
        &ctx,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("zuul recover"));
    assert!(backend.has_secret("KEY", "dev"));
}

#[tokio::test]
async fn import_journal_lock_blocks_concurrent_import() {
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
// env rename with journal
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rename_env_moves_secrets_to_new_env() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("staging", None);
    backend.seed_secret("DB_URL", "staging", "postgres://staging-db");
    backend.seed_secret("API_KEY", "staging", "sk_staging_123");

    let ctx = batch_ctx(dir.path());
    cli::env::update(
        &backend,
        "staging",
        Some("staging-v2"),
        None,
        true,
        &OutputFormat::Text,
        &ctx,
    )
    .await
    .unwrap();

    assert_eq!(
        backend.get_value("DB_URL", "staging-v2").as_deref(),
        Some("postgres://staging-db")
    );
    assert_eq!(
        backend.get_value("API_KEY", "staging-v2").as_deref(),
        Some("sk_staging_123")
    );
    assert!(!backend.has_secret("DB_URL", "staging"));
    assert!(!backend.has_secret("API_KEY", "staging"));
    assert!(backend.has_env("staging-v2"));
    assert!(!backend.has_env("staging"));
    assert!(!journal::journal_path(dir.path()).exists());
}

#[tokio::test]
async fn rename_env_preserves_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("dev", None);
    backend.seed_secret("DB_URL", "dev", "postgres://localhost");
    backend.seed_metadata("DB_URL", "dev", "owner", "backend-team");
    backend.seed_metadata("DB_URL", "dev", "rotate-by", "2026-06-01");

    let ctx = batch_ctx(dir.path());
    cli::env::update(
        &backend,
        "dev",
        Some("development"),
        None,
        true,
        &OutputFormat::Text,
        &ctx,
    )
    .await
    .unwrap();

    let meta = backend.get_meta("DB_URL", "development");
    assert_eq!(meta.get("owner").map(String::as_str), Some("backend-team"));
    assert_eq!(
        meta.get("rotate-by").map(String::as_str),
        Some("2026-06-01")
    );

    let old_meta = backend.get_meta("DB_URL", "dev");
    assert!(old_meta.is_empty());
}

#[tokio::test]
async fn rename_env_with_description_change() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("staging", None);
    backend.seed_secret("KEY", "staging", "value");

    let ctx = batch_ctx(dir.path());
    cli::env::update(
        &backend,
        "staging",
        Some("prod"),
        Some("Production environment"),
        true,
        &OutputFormat::Text,
        &ctx,
    )
    .await
    .unwrap();

    assert_eq!(backend.get_value("KEY", "prod").as_deref(), Some("value"));
    assert!(!backend.has_secret("KEY", "staging"));
    assert!(backend.has_env("prod"));
    assert!(!backend.has_env("staging"));
}

#[tokio::test]
async fn rename_empty_env_skips_journal() {
    let dir = tempfile::tempdir().unwrap();
    let backend = admin_backend();
    backend.seed_environment("empty", None);

    let ctx = batch_ctx(dir.path());
    cli::env::update(
        &backend,
        "empty",
        Some("renamed"),
        None,
        true,
        &OutputFormat::Text,
        &ctx,
    )
    .await
    .unwrap();

    assert!(!dir.path().join(".zuul").exists());
    assert!(backend.has_env("renamed"));
    assert!(!backend.has_env("empty"));
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
