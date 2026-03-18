//! Access control tests exercising CLI handlers through the MockBackend
//! with configurable IAM-like access rules.

use std::collections::HashMap;

use zuul::backend::Backend;
use zuul::cli::{self, ExportFormat, OutputFormat};
use zuul::config::Config;
use zuul::error::ZuulError;
use zuul::progress::{BatchContext, ProgressOpts};

use super::common::{AccessLevel, MockBackend};

const PROGRESS: ProgressOpts = ProgressOpts {
    non_interactive: true,
};

// ---------------------------------------------------------------------------
// Admin: full access through CLI handlers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_can_list_environments() {
    let backend = MockBackend::new(AccessLevel::Admin);
    backend.seed_environment("dev", Some("Development"));
    backend.seed_environment("production", Some("Production"));

    cli::env::list(&backend, &OutputFormat::Text).await.unwrap();
}

#[tokio::test]
async fn admin_can_get_production_secret() {
    let backend = MockBackend::new(AccessLevel::Admin);
    backend.seed_environment("production", None);
    backend.seed_secret("DB_URL", "production", "prod://db");

    cli::secret::get(&backend, "DB_URL", Some("production"), PROGRESS)
        .await
        .unwrap();
}

#[tokio::test]
async fn admin_can_clear_environment() {
    let backend = MockBackend::new(AccessLevel::Admin);
    backend.seed_environment("staging", None);
    backend.seed_secret("DB_URL", "staging", "postgres://staging");
    backend.seed_secret("API_KEY", "staging", "sk_staging");

    let ctx = BatchContext {
        progress: PROGRESS,
        project_root: None,
    };
    cli::env::clear(&backend, "staging", true, false, &OutputFormat::Text, &ctx)
        .await
        .unwrap();

    assert!(!backend.has_secret("DB_URL", "staging"));
    assert!(!backend.has_secret("API_KEY", "staging"));
    // Environment still exists after clear.
    backend.get_environment("staging").await.unwrap();
}

// ---------------------------------------------------------------------------
// Dev-scoped: can access dev, denied on production — through CLI handlers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dev_scoped_can_get_dev_secret_via_handler() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));
    backend.seed_environment("dev", None);
    backend.seed_secret("API_KEY", "dev", "dev_key_123");

    cli::secret::get(&backend, "API_KEY", Some("dev"), PROGRESS)
        .await
        .unwrap();
}

#[tokio::test]
async fn dev_scoped_get_production_secret_denied_via_handler() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));
    backend.seed_environment("production", None);
    backend.seed_secret("DB_URL", "production", "prod://db");

    let err = cli::secret::get(&backend, "DB_URL", Some("production"), PROGRESS)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ZuulError::PermissionDenied { .. }),
        "CLI handler should propagate PermissionDenied, got: {err}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("Permission denied") && msg.contains("permissions"),
        "msg: {msg}"
    );
}

#[tokio::test]
async fn dev_scoped_list_secrets_filters_out_production() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));
    backend.seed_environment("dev", None);
    backend.seed_environment("production", None);
    backend.seed_secret("DEV_KEY", "dev", "d");
    backend.seed_secret("PROD_KEY", "production", "p");

    let entries = backend.list_secrets(None).await.unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"DEV_KEY"));
    assert!(!names.contains(&"PROD_KEY"));
}

#[tokio::test]
async fn dev_scoped_export_production_denied_via_handler() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));
    backend.seed_environment("production", None);

    let config = Config {
        backend_type: "mock".to_string(),
        project_id: Some("test".to_string()),
        credentials: None,
        default_environment: Some("production".to_string()),
        local_overrides: HashMap::new(),
        config_dir: None,
        file_path: None,
        identity: None,
    };

    let err = cli::export::run(
        &backend,
        &config,
        "production",
        &ExportFormat::Json,
        None,
        true,
        PROGRESS,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, ZuulError::PermissionDenied { .. }),
        "export handler should propagate PermissionDenied, got: {err}"
    );
}

#[tokio::test]
async fn dev_scoped_env_list_fails_when_unauthorized_envs_exist() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));
    backend.seed_environment("dev", None);
    backend.seed_environment("production", None);

    let err = cli::env::list(&backend, &OutputFormat::Text)
        .await
        .unwrap_err();
    assert!(matches!(err, ZuulError::PermissionDenied { .. }));
}

#[tokio::test]
async fn dev_scoped_cannot_set_production_metadata_via_handler() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));
    backend.seed_environment("production", None);
    backend.seed_secret("KEY", "production", "val");

    let ctx = BatchContext {
        progress: PROGRESS,
        project_root: None,
    };
    let err = cli::metadata::set(&backend, "KEY", Some("production"), "owner", "me", &ctx)
        .await
        .unwrap_err();
    assert!(matches!(err, ZuulError::PermissionDenied { .. }));
}

// ---------------------------------------------------------------------------
// Multi-env scoped identity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multi_env_scoped_access_via_handlers() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec![
        "dev".to_string(),
        "staging".to_string(),
    ]));
    backend.seed_environment("dev", None);
    backend.seed_environment("staging", None);
    backend.seed_environment("production", None);
    backend.seed_secret("KEY", "dev", "d");
    backend.seed_secret("KEY", "staging", "s");
    backend.seed_secret("KEY", "production", "p");

    cli::secret::get(&backend, "KEY", Some("dev"), PROGRESS)
        .await
        .unwrap();
    cli::secret::get(&backend, "KEY", Some("staging"), PROGRESS)
        .await
        .unwrap();

    let err = cli::secret::get(&backend, "KEY", Some("production"), PROGRESS)
        .await
        .unwrap_err();
    assert!(matches!(err, ZuulError::PermissionDenied { .. }));
}

// ---------------------------------------------------------------------------
// Unauthenticated: all operations fail with Auth error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unauthenticated_list_envs_fails_via_handler() {
    let backend = MockBackend::new(AccessLevel::Unauthenticated);

    let err = cli::env::list(&backend, &OutputFormat::Text)
        .await
        .unwrap_err();
    assert!(matches!(err, ZuulError::Auth(_)));
    let msg = err.to_string();
    assert!(
        msg.contains("zuul auth"),
        "should suggest zuul auth, got: {msg}"
    );
}

#[tokio::test]
async fn unauthenticated_get_secret_fails_via_handler() {
    let backend = MockBackend::new(AccessLevel::Unauthenticated);
    backend.seed_environment("dev", None);
    backend.seed_secret("KEY", "dev", "val");

    let err = cli::secret::get(&backend, "KEY", Some("dev"), PROGRESS)
        .await
        .unwrap_err();
    assert!(matches!(err, ZuulError::Auth(_)));
}

// ---------------------------------------------------------------------------
// Error messages: user-facing quality through CLI handler propagation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn permission_denied_message_contains_resource_and_iam_hint() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));
    backend.seed_environment("production", None);
    backend.seed_secret("SECRET", "production", "val");

    let err = cli::secret::get(&backend, "SECRET", Some("production"), PROGRESS)
        .await
        .unwrap_err();
    let msg = err.to_string();

    assert!(msg.contains("Permission denied"), "msg: {msg}");
    assert!(msg.contains("production"), "should reference env: {msg}");
    assert!(
        msg.contains("permissions"),
        "should mention permissions: {msg}"
    );
}

#[tokio::test]
async fn auth_error_message_suggests_zuul_auth() {
    let backend = MockBackend::new(AccessLevel::Unauthenticated);

    let err = cli::env::list(&backend, &OutputFormat::Text)
        .await
        .unwrap_err();
    let msg = err.to_string();

    assert!(msg.contains("zuul auth"), "got: {msg}");
}

#[tokio::test]
async fn permission_denied_does_not_reveal_secret_value() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));
    backend.seed_secret("DB_PASSWORD", "production", "super_secret_password_123");

    let err = cli::secret::get(&backend, "DB_PASSWORD", Some("production"), PROGRESS)
        .await
        .unwrap_err();
    let msg = err.to_string();

    assert!(
        !msg.contains("super_secret_password_123"),
        "error message must not leak secret value, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Cross-environment metadata operations with journaling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cross_env_metadata_set_stops_on_first_failure() {
    // "staging" is configured to fail metadata writes.
    // Environments are sorted alphabetically by list_secrets, so dev succeeds
    // before staging fails.
    let backend =
        MockBackend::with_failing_metadata_envs(AccessLevel::Admin, vec!["staging".to_string()]);
    backend.seed_environment("dev", None);
    backend.seed_environment("staging", None);
    backend.seed_secret("KEY", "dev", "d");
    backend.seed_secret("KEY", "staging", "s");

    let ctx = BatchContext {
        progress: PROGRESS,
        project_root: None,
    };
    let err = cli::metadata::set(&backend, "KEY", None, "owner", "team", &ctx)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("staging"), "should mention staging: {msg}");

    // dev should have succeeded before the failure.
    let meta = backend.get_metadata("KEY", "dev").await.unwrap();
    assert_eq!(meta.get("owner").map(String::as_str), Some("team"));
}

#[tokio::test]
async fn cross_env_metadata_delete_stops_on_first_failure() {
    let backend =
        MockBackend::with_failing_metadata_envs(AccessLevel::Admin, vec!["production".to_string()]);
    backend.seed_environment("dev", None);
    backend.seed_environment("production", None);
    backend.seed_secret("KEY", "dev", "d");
    backend.seed_secret("KEY", "production", "p");

    backend
        .set_metadata("KEY", "dev", "tag", "v1")
        .await
        .unwrap();

    let ctx = BatchContext {
        progress: PROGRESS,
        project_root: None,
    };
    let err = cli::metadata::delete(&backend, "KEY", None, "tag", &ctx)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("production"),
        "should mention production: {msg}"
    );

    // dev should have succeeded before the failure.
    let meta = backend.get_metadata("KEY", "dev").await.unwrap();
    assert!(!meta.contains_key("tag"), "tag should be deleted from dev");
}

#[tokio::test]
async fn cross_env_metadata_set_all_succeed() {
    let backend = MockBackend::new(AccessLevel::Admin);
    backend.seed_environment("dev", None);
    backend.seed_environment("staging", None);
    backend.seed_secret("KEY", "dev", "d");
    backend.seed_secret("KEY", "staging", "s");

    let ctx = BatchContext {
        progress: PROGRESS,
        project_root: None,
    };
    cli::metadata::set(&backend, "KEY", None, "owner", "ops", &ctx)
        .await
        .unwrap();

    let dev_meta = backend.get_metadata("KEY", "dev").await.unwrap();
    let stg_meta = backend.get_metadata("KEY", "staging").await.unwrap();
    assert_eq!(dev_meta.get("owner").map(String::as_str), Some("ops"));
    assert_eq!(stg_meta.get("owner").map(String::as_str), Some("ops"));
}

#[tokio::test]
async fn cross_env_metadata_single_env_still_fails_fast() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));
    backend.seed_environment("production", None);
    backend.seed_secret("KEY", "production", "p");

    let ctx = BatchContext {
        progress: PROGRESS,
        project_root: None,
    };
    let err = cli::metadata::set(&backend, "KEY", Some("production"), "owner", "me", &ctx)
        .await
        .unwrap_err();
    assert!(matches!(err, ZuulError::PermissionDenied { .. }));
}
