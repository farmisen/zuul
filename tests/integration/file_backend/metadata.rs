//! Metadata CRUD tests for the file backend.

use zuul::backend::file_backend::FileBackend;
use zuul::backend::Backend;
use zuul::error::ZuulError;

fn test_backend(dir: &std::path::Path) -> FileBackend {
    // SAFETY: tests run sequentially within this binary (no parallel env var mutation).
    unsafe { std::env::set_var("ZUUL_PASSPHRASE", "test-passphrase") };
    FileBackend::new(dir.join(".zuul.secrets.enc"), None)
}

/// Create a backend with a pre-seeded environment and secret.
async fn seeded_backend(dir: &std::path::Path) -> FileBackend {
    let backend = test_backend(dir);
    backend.create_environment("dev", None).await.unwrap();
    backend
        .set_secret("DB_URL", "dev", "postgres://localhost")
        .await
        .unwrap();
    backend
}

// ---------------------------------------------------------------------------
// set + get metadata
// ---------------------------------------------------------------------------

#[tokio::test]
async fn set_and_get_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    backend
        .set_metadata("DB_URL", "dev", "owner", "backend-team")
        .await
        .unwrap();

    let meta = backend.get_metadata("DB_URL", "dev").await.unwrap();
    assert_eq!(meta.get("owner").map(String::as_str), Some("backend-team"));
}

#[tokio::test]
async fn set_metadata_overwrites_existing_key() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    backend
        .set_metadata("DB_URL", "dev", "owner", "old-team")
        .await
        .unwrap();
    backend
        .set_metadata("DB_URL", "dev", "owner", "new-team")
        .await
        .unwrap();

    let meta = backend.get_metadata("DB_URL", "dev").await.unwrap();
    assert_eq!(meta.get("owner").map(String::as_str), Some("new-team"));
}

#[tokio::test]
async fn get_metadata_empty() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    let meta = backend.get_metadata("DB_URL", "dev").await.unwrap();
    assert!(meta.is_empty());
}

#[tokio::test]
async fn get_metadata_nonexistent_secret() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());
    backend.create_environment("dev", None).await.unwrap();

    let err = backend
        .get_metadata("NOPE", "dev")
        .await
        .unwrap_err();
    assert!(
        matches!(err, ZuulError::NotFound { .. }),
        "expected NotFound, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// delete metadata
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_metadata_basic() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    backend
        .set_metadata("DB_URL", "dev", "owner", "team")
        .await
        .unwrap();
    backend
        .delete_metadata("DB_URL", "dev", "owner")
        .await
        .unwrap();

    let meta = backend.get_metadata("DB_URL", "dev").await.unwrap();
    assert!(!meta.contains_key("owner"));
}

#[tokio::test]
async fn delete_metadata_nonexistent_key_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    // Deleting a key that doesn't exist should not error.
    backend
        .delete_metadata("DB_URL", "dev", "nonexistent")
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// multiple metadata keys
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multiple_metadata_keys() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    backend
        .set_metadata("DB_URL", "dev", "owner", "backend-team")
        .await
        .unwrap();
    backend
        .set_metadata("DB_URL", "dev", "rotate-by", "2026-06-01")
        .await
        .unwrap();
    backend
        .set_metadata("DB_URL", "dev", "source", "AWS RDS")
        .await
        .unwrap();

    let meta = backend.get_metadata("DB_URL", "dev").await.unwrap();
    assert_eq!(meta.len(), 3);
    assert_eq!(meta.get("owner").map(String::as_str), Some("backend-team"));
    assert_eq!(
        meta.get("rotate-by").map(String::as_str),
        Some("2026-06-01")
    );
    assert_eq!(meta.get("source").map(String::as_str), Some("AWS RDS"));
}

// ---------------------------------------------------------------------------
// metadata isolation across environments
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metadata_isolated_across_environments() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());
    backend.create_environment("dev", None).await.unwrap();
    backend.create_environment("prod", None).await.unwrap();
    backend
        .set_secret("KEY", "dev", "d")
        .await
        .unwrap();
    backend
        .set_secret("KEY", "prod", "p")
        .await
        .unwrap();

    backend
        .set_metadata("KEY", "dev", "owner", "dev-team")
        .await
        .unwrap();
    backend
        .set_metadata("KEY", "prod", "owner", "ops-team")
        .await
        .unwrap();

    let dev_meta = backend.get_metadata("KEY", "dev").await.unwrap();
    let prod_meta = backend.get_metadata("KEY", "prod").await.unwrap();
    assert_eq!(dev_meta.get("owner").map(String::as_str), Some("dev-team"));
    assert_eq!(
        prod_meta.get("owner").map(String::as_str),
        Some("ops-team")
    );
}

// ---------------------------------------------------------------------------
// metadata deleted with secret
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_secret_removes_its_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    backend
        .set_metadata("DB_URL", "dev", "owner", "team")
        .await
        .unwrap();

    backend.delete_secret("DB_URL", "dev").await.unwrap();

    // Re-create the secret — metadata should be gone
    backend
        .set_secret("DB_URL", "dev", "new-value")
        .await
        .unwrap();
    let meta = backend.get_metadata("DB_URL", "dev").await.unwrap();
    assert!(meta.is_empty());
}
