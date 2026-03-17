//! Environment CRUD tests for the file backend.

use zuul::backend::Backend;
use zuul::backend::file_backend::FileBackend;
use zuul::error::ZuulError;

fn test_backend(dir: &std::path::Path) -> FileBackend {
    // SAFETY: tests run sequentially within this binary (no parallel env var mutation).
    unsafe { std::env::set_var("ZUUL_PASSPHRASE", "test-passphrase") };
    FileBackend::new(dir.join(".zuul.secrets.enc"), None)
}

// ---------------------------------------------------------------------------
// create environment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_environment_basic() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    let env = backend
        .create_environment("dev", Some("Development"))
        .await
        .unwrap();

    assert_eq!(env.name, "dev");
    assert_eq!(env.description.as_deref(), Some("Development"));
}

#[tokio::test]
async fn create_environment_without_description() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    let env = backend.create_environment("staging", None).await.unwrap();

    assert_eq!(env.name, "staging");
    assert_eq!(env.description, None);
}

#[tokio::test]
async fn create_duplicate_environment_fails() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    backend.create_environment("dev", None).await.unwrap();
    let err = backend.create_environment("dev", None).await.unwrap_err();

    assert!(
        matches!(err, ZuulError::AlreadyExists { .. }),
        "expected AlreadyExists, got: {err}"
    );
}

#[tokio::test]
async fn create_environment_validates_name() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    let err = backend
        .create_environment("INVALID", None)
        .await
        .unwrap_err();
    assert!(matches!(err, ZuulError::Validation(_)));

    let err = backend
        .create_environment("has__double", None)
        .await
        .unwrap_err();
    assert!(matches!(err, ZuulError::Validation(_)));
}

// ---------------------------------------------------------------------------
// list environments
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_environments_empty() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    let envs = backend.list_environments().await.unwrap();
    assert!(envs.is_empty());
}

#[tokio::test]
async fn list_environments_returns_sorted() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    backend.create_environment("staging", None).await.unwrap();
    backend.create_environment("dev", None).await.unwrap();
    backend
        .create_environment("production", None)
        .await
        .unwrap();

    let envs = backend.list_environments().await.unwrap();
    let names: Vec<&str> = envs.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["dev", "production", "staging"]);
}

// ---------------------------------------------------------------------------
// get environment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_environment_exists() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    backend
        .create_environment("dev", Some("Development"))
        .await
        .unwrap();

    let env = backend.get_environment("dev").await.unwrap();
    assert_eq!(env.name, "dev");
    assert_eq!(env.description.as_deref(), Some("Development"));
}

#[tokio::test]
async fn get_environment_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    let err = backend.get_environment("nope").await.unwrap_err();
    assert!(
        matches!(err, ZuulError::NotFound { .. }),
        "expected NotFound, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// update environment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_environment_description() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    backend.create_environment("dev", None).await.unwrap();
    let env = backend
        .update_environment("dev", None, Some("Updated"))
        .await
        .unwrap();

    assert_eq!(env.name, "dev");
    assert_eq!(env.description.as_deref(), Some("Updated"));

    // Verify persisted
    let env = backend.get_environment("dev").await.unwrap();
    assert_eq!(env.description.as_deref(), Some("Updated"));
}

#[tokio::test]
async fn update_environment_rename() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    backend.create_environment("old", None).await.unwrap();
    let env = backend
        .update_environment("old", Some("new"), None)
        .await
        .unwrap();

    assert_eq!(env.name, "new");

    // Old name should be gone
    let err = backend.get_environment("old").await.unwrap_err();
    assert!(matches!(err, ZuulError::NotFound { .. }));

    // New name should exist
    backend.get_environment("new").await.unwrap();
}

#[tokio::test]
async fn update_environment_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    let err = backend
        .update_environment("nope", None, Some("desc"))
        .await
        .unwrap_err();
    assert!(matches!(err, ZuulError::NotFound { .. }));
}

// ---------------------------------------------------------------------------
// delete environment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_environment_basic() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    backend.create_environment("dev", None).await.unwrap();
    backend.delete_environment("dev").await.unwrap();

    let err = backend.get_environment("dev").await.unwrap_err();
    assert!(matches!(err, ZuulError::NotFound { .. }));
}

#[tokio::test]
async fn delete_environment_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    let err = backend.delete_environment("nope").await.unwrap_err();
    assert!(matches!(err, ZuulError::NotFound { .. }));
}

#[tokio::test]
async fn delete_environment_cascades_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let backend = test_backend(dir.path());

    backend.create_environment("dev", None).await.unwrap();
    backend
        .set_secret("DB_URL", "dev", "postgres://localhost")
        .await
        .unwrap();
    backend
        .set_secret("API_KEY", "dev", "sk_test")
        .await
        .unwrap();

    backend.delete_environment("dev").await.unwrap();

    // Environment gone
    assert!(matches!(
        backend.get_environment("dev").await,
        Err(ZuulError::NotFound { .. })
    ));

    // Secrets also gone (recreate env to check)
    backend.create_environment("dev", None).await.unwrap();
    let secrets = backend.list_secrets(Some("dev")).await.unwrap();
    assert!(secrets.is_empty());
}
