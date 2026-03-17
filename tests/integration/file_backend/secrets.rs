//! Secret CRUD tests for the file backend.

use zuul::backend::Backend;
use zuul::backend::file_backend::FileBackend;
use zuul::error::ZuulError;

fn test_backend(dir: &std::path::Path) -> FileBackend {
    // SAFETY: tests run sequentially within this binary (no parallel env var mutation).
    unsafe { std::env::set_var("ZUUL_PASSPHRASE", "test-passphrase") };
    FileBackend::new(dir.join(".zuul.secrets.enc"), None)
}

/// Create a backend with a pre-seeded environment.
async fn seeded_backend(dir: &std::path::Path) -> FileBackend {
    let backend = test_backend(dir);
    backend.create_environment("dev", None).await.unwrap();
    backend
}

// ---------------------------------------------------------------------------
// set + get secret
// ---------------------------------------------------------------------------

#[tokio::test]
async fn set_and_get_secret() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    backend
        .set_secret("DB_URL", "dev", "postgres://localhost")
        .await
        .unwrap();

    let sv = backend.get_secret("DB_URL", "dev").await.unwrap();
    assert_eq!(sv.name, "DB_URL");
    assert_eq!(sv.environment, "dev");
    assert_eq!(sv.value, "postgres://localhost");
}

#[tokio::test]
async fn set_secret_overwrites_existing() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    backend
        .set_secret("DB_URL", "dev", "old-value")
        .await
        .unwrap();
    backend
        .set_secret("DB_URL", "dev", "new-value")
        .await
        .unwrap();

    let sv = backend.get_secret("DB_URL", "dev").await.unwrap();
    assert_eq!(sv.value, "new-value");
}

#[tokio::test]
async fn set_secret_increments_version() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    backend.set_secret("KEY", "dev", "v1").await.unwrap();
    let sv1 = backend.get_secret("KEY", "dev").await.unwrap();

    backend.set_secret("KEY", "dev", "v2").await.unwrap();
    let sv2 = backend.get_secret("KEY", "dev").await.unwrap();

    assert_ne!(sv1.version, sv2.version);
}

#[tokio::test]
async fn set_secret_nonexistent_env_fails() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    let err = backend.set_secret("KEY", "nope", "val").await.unwrap_err();
    assert!(
        matches!(err, ZuulError::NotFound { .. }),
        "expected NotFound, got: {err}"
    );
}

#[tokio::test]
async fn get_secret_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    let err = backend.get_secret("NOPE", "dev").await.unwrap_err();
    assert!(matches!(err, ZuulError::NotFound { .. }));
}

#[tokio::test]
async fn set_and_get_multiline_value() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    let cert = "-----BEGIN CERT-----\nMIIBxTCCA...\n-----END CERT-----";
    backend.set_secret("TLS_CERT", "dev", cert).await.unwrap();

    let sv = backend.get_secret("TLS_CERT", "dev").await.unwrap();
    assert_eq!(sv.value, cert);
}

// ---------------------------------------------------------------------------
// delete secret
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_secret_basic() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    backend.set_secret("KEY", "dev", "val").await.unwrap();
    backend.delete_secret("KEY", "dev").await.unwrap();

    let err = backend.get_secret("KEY", "dev").await.unwrap_err();
    assert!(matches!(err, ZuulError::NotFound { .. }));
}

#[tokio::test]
async fn delete_secret_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    let err = backend.delete_secret("NOPE", "dev").await.unwrap_err();
    assert!(matches!(err, ZuulError::NotFound { .. }));
}

// ---------------------------------------------------------------------------
// list secrets
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_secrets_empty() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    let secrets = backend.list_secrets(Some("dev")).await.unwrap();
    assert!(secrets.is_empty());
}

#[tokio::test]
async fn list_secrets_filtered_by_env() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());
    backend.create_environment("dev", None).await.unwrap();
    backend.create_environment("staging", None).await.unwrap();

    backend.set_secret("A", "dev", "a").await.unwrap();
    backend.set_secret("B", "dev", "b").await.unwrap();
    backend.set_secret("C", "staging", "c").await.unwrap();

    let dev_secrets = backend.list_secrets(Some("dev")).await.unwrap();
    let names: Vec<&str> = dev_secrets.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"A"));
    assert!(names.contains(&"B"));
    assert!(!names.contains(&"C"));
}

#[tokio::test]
async fn list_secrets_all_environments() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());
    backend.create_environment("dev", None).await.unwrap();
    backend.create_environment("staging", None).await.unwrap();

    backend.set_secret("SHARED", "dev", "d").await.unwrap();
    backend.set_secret("SHARED", "staging", "s").await.unwrap();
    backend.set_secret("DEV_ONLY", "dev", "d").await.unwrap();

    let all = backend.list_secrets(None).await.unwrap();
    let shared = all.iter().find(|s| s.name == "SHARED").unwrap();
    assert_eq!(shared.environments.len(), 2);

    let dev_only = all.iter().find(|s| s.name == "DEV_ONLY").unwrap();
    assert_eq!(dev_only.environments.len(), 1);
}

// ---------------------------------------------------------------------------
// list secrets for environment (with values)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_secrets_for_environment() {
    let dir = tempfile::tempdir().unwrap();
    let backend = seeded_backend(dir.path()).await;

    backend.set_secret("A", "dev", "val_a").await.unwrap();
    backend.set_secret("B", "dev", "val_b").await.unwrap();

    let pairs = backend.list_secrets_for_environment("dev").await.unwrap();
    assert_eq!(pairs.len(), 2);

    let a = pairs.iter().find(|(n, _)| n == "A").unwrap();
    assert_eq!(a.1.value, "val_a");

    let b = pairs.iter().find(|(n, _)| n == "B").unwrap();
    assert_eq!(b.1.value, "val_b");
}

// ---------------------------------------------------------------------------
// cross-environment secret isolation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn secrets_isolated_across_environments() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());
    backend.create_environment("dev", None).await.unwrap();
    backend.create_environment("prod", None).await.unwrap();

    backend.set_secret("DB_URL", "dev", "dev-db").await.unwrap();
    backend
        .set_secret("DB_URL", "prod", "prod-db")
        .await
        .unwrap();

    let dev = backend.get_secret("DB_URL", "dev").await.unwrap();
    let prod = backend.get_secret("DB_URL", "prod").await.unwrap();
    assert_eq!(dev.value, "dev-db");
    assert_eq!(prod.value, "prod-db");

    // Delete from dev doesn't affect prod
    backend.delete_secret("DB_URL", "dev").await.unwrap();
    let prod = backend.get_secret("DB_URL", "prod").await.unwrap();
    assert_eq!(prod.value, "prod-db");
}

// ---------------------------------------------------------------------------
// persistence across backend instances
// ---------------------------------------------------------------------------

#[tokio::test]
async fn data_persists_across_instances() {
    let dir = tempfile::tempdir().unwrap();

    {
        let backend = test_backend(dir.path());
        backend
            .create_environment("dev", Some("Development"))
            .await
            .unwrap();
        backend.set_secret("KEY", "dev", "persisted").await.unwrap();
    }

    // New instance reads from the same file
    let backend = test_backend(dir.path());
    let env = backend.get_environment("dev").await.unwrap();
    assert_eq!(env.description.as_deref(), Some("Development"));

    let sv = backend.get_secret("KEY", "dev").await.unwrap();
    assert_eq!(sv.value, "persisted");
}
