//! Backend-level access control tests using a mock backend.
//!
//! These tests exercise actual CLI handler functions (env, secret, export, metadata)
//! through a `MockBackend` with configurable IAM-like access rules, verifying that
//! `PermissionDenied` and `Auth` errors propagate correctly with actionable messages.
//!
//! No emulator or GCP credentials needed — runs with `cargo test`.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::Utc;

use zuul::backend::Backend;
use zuul::cli::{self, ExportFormat, OutputFormat};
use zuul::config::Config;
use zuul::error::{ResourceType, ZuulError};
use zuul::models::{Environment, SecretEntry, SecretValue};
use zuul::progress::ProgressOpts;

const PROGRESS: ProgressOpts = ProgressOpts {
    non_interactive: true,
};

// ---------------------------------------------------------------------------
// MockBackend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum AccessLevel {
    Admin,
    Scoped(Vec<String>),
    Unauthenticated,
}

struct MockState {
    environments: HashMap<String, Environment>,
    secrets: HashMap<(String, String), SecretValue>,
    metadata: HashMap<(String, String), HashMap<String, String>>,
}

struct MockBackend {
    access: AccessLevel,
    state: Mutex<MockState>,
}

impl MockBackend {
    fn new(access: AccessLevel) -> Self {
        Self {
            access,
            state: Mutex::new(MockState {
                environments: HashMap::new(),
                secrets: HashMap::new(),
                metadata: HashMap::new(),
            }),
        }
    }

    fn seed_environment(&self, name: &str, description: Option<&str>) {
        let now = Utc::now();
        let env = Environment {
            name: name.to_string(),
            description: description.map(String::from),
            created_at: now,
            updated_at: now,
        };
        self.state
            .lock()
            .unwrap()
            .environments
            .insert(name.to_string(), env);
    }

    fn seed_secret(&self, name: &str, environment: &str, value: &str) {
        let now = Utc::now();
        let sv = SecretValue {
            name: name.to_string(),
            environment: environment.to_string(),
            value: value.to_string(),
            version: "1".to_string(),
            created_at: now,
            updated_at: now,
        };
        self.state
            .lock()
            .unwrap()
            .secrets
            .insert((name.to_string(), environment.to_string()), sv);
    }

    fn check_env_access(&self, environment: &str) -> Result<(), ZuulError> {
        match &self.access {
            AccessLevel::Admin => Ok(()),
            AccessLevel::Scoped(allowed) => {
                if allowed.iter().any(|e| e == environment) {
                    Ok(())
                } else {
                    Err(ZuulError::PermissionDenied {
                        resource: format!("zuul__{environment}__*"),
                    })
                }
            }
            AccessLevel::Unauthenticated => {
                Err(ZuulError::Auth("No valid credentials found.".to_string()))
            }
        }
    }

    fn check_general_access(&self) -> Result<(), ZuulError> {
        match &self.access {
            AccessLevel::Admin | AccessLevel::Scoped(_) => Ok(()),
            AccessLevel::Unauthenticated => {
                Err(ZuulError::Auth("No valid credentials found.".to_string()))
            }
        }
    }

    fn check_admin_access(&self) -> Result<(), ZuulError> {
        match &self.access {
            AccessLevel::Admin => Ok(()),
            AccessLevel::Scoped(_) => Err(ZuulError::PermissionDenied {
                resource: "zuul__registry".to_string(),
            }),
            AccessLevel::Unauthenticated => {
                Err(ZuulError::Auth("No valid credentials found.".to_string()))
            }
        }
    }
}

impl Backend for MockBackend {
    fn list_environments(
        &self,
    ) -> impl Future<Output = Result<Vec<Environment>, ZuulError>> + Send {
        let result = self.check_general_access().map(|()| {
            let state = self.state.lock().unwrap();
            state.environments.values().cloned().collect()
        });
        async move { result }
    }

    fn create_environment(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> impl Future<Output = Result<Environment, ZuulError>> + Send {
        let result = self.check_admin_access().and_then(|()| {
            let mut state = self.state.lock().unwrap();
            if state.environments.contains_key(name) {
                return Err(ZuulError::AlreadyExists {
                    resource_type: ResourceType::Environment,
                    name: name.to_string(),
                    environment: None,
                });
            }
            let now = Utc::now();
            let env = Environment {
                name: name.to_string(),
                description: description.map(String::from),
                created_at: now,
                updated_at: now,
            };
            state.environments.insert(name.to_string(), env.clone());
            Ok(env)
        });
        async move { result }
    }

    fn get_environment(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Environment, ZuulError>> + Send {
        let result = self.check_general_access().and_then(|()| {
            let state = self.state.lock().unwrap();
            state
                .environments
                .get(name)
                .cloned()
                .ok_or_else(|| ZuulError::NotFound {
                    resource_type: ResourceType::Environment,
                    name: name.to_string(),
                    environment: None,
                })
        });
        async move { result }
    }

    fn update_environment(
        &self,
        name: &str,
        new_name: Option<&str>,
        new_description: Option<&str>,
    ) -> impl Future<Output = Result<Environment, ZuulError>> + Send {
        let result = self.check_admin_access().and_then(|()| {
            let mut state = self.state.lock().unwrap();
            let mut env = state
                .environments
                .remove(name)
                .ok_or_else(|| ZuulError::NotFound {
                    resource_type: ResourceType::Environment,
                    name: name.to_string(),
                    environment: None,
                })?;
            let final_name = new_name.unwrap_or(name);
            if let Some(desc) = new_description {
                env.description = Some(desc.to_string());
            }
            env.name = final_name.to_string();
            env.updated_at = Utc::now();
            state
                .environments
                .insert(final_name.to_string(), env.clone());
            Ok(env)
        });
        async move { result }
    }

    fn delete_environment(&self, name: &str) -> impl Future<Output = Result<(), ZuulError>> + Send {
        let result = self.check_admin_access().and_then(|()| {
            let mut state = self.state.lock().unwrap();
            if !state.environments.contains_key(name) {
                return Err(ZuulError::NotFound {
                    resource_type: ResourceType::Environment,
                    name: name.to_string(),
                    environment: None,
                });
            }
            state.environments.remove(name);
            state.secrets.retain(|(_, env), _| env != name);
            state.metadata.retain(|(_, env), _| env != name);
            Ok(())
        });
        async move { result }
    }

    fn list_secrets(
        &self,
        environment: Option<&str>,
    ) -> impl Future<Output = Result<Vec<SecretEntry>, ZuulError>> + Send {
        let result = (|| -> Result<Vec<SecretEntry>, ZuulError> {
            if let Some(env) = environment {
                self.check_env_access(env)?;
            } else {
                self.check_general_access()?;
            }
            let state = self.state.lock().unwrap();
            let mut entries: HashMap<String, Vec<String>> = HashMap::new();
            for (name, env) in state.secrets.keys() {
                if let Some(filter_env) = environment
                    && env != filter_env
                {
                    continue;
                }
                if let AccessLevel::Scoped(allowed) = &self.access
                    && !allowed.iter().any(|a| a == env)
                {
                    continue;
                }
                entries.entry(name.clone()).or_default().push(env.clone());
            }
            Ok(entries
                .into_iter()
                .map(|(name, environments)| SecretEntry {
                    name,
                    environments,
                    metadata: HashMap::new(),
                })
                .collect())
        })();
        async move { result }
    }

    fn get_secret(
        &self,
        name: &str,
        environment: &str,
    ) -> impl Future<Output = Result<SecretValue, ZuulError>> + Send {
        let result = self.check_env_access(environment).and_then(|()| {
            let state = self.state.lock().unwrap();
            state
                .secrets
                .get(&(name.to_string(), environment.to_string()))
                .cloned()
                .ok_or_else(|| ZuulError::NotFound {
                    resource_type: ResourceType::Secret,
                    name: name.to_string(),
                    environment: Some(environment.to_string()),
                })
        });
        async move { result }
    }

    fn set_secret(
        &self,
        name: &str,
        environment: &str,
        value: &str,
    ) -> impl Future<Output = Result<(), ZuulError>> + Send {
        let result = self.check_env_access(environment).map(|()| {
            self.seed_secret(name, environment, value);
        });
        async move { result }
    }

    fn delete_secret(
        &self,
        name: &str,
        environment: &str,
    ) -> impl Future<Output = Result<(), ZuulError>> + Send {
        let result = self.check_env_access(environment).and_then(|()| {
            let mut state = self.state.lock().unwrap();
            let key = (name.to_string(), environment.to_string());
            if state.secrets.remove(&key).is_none() {
                return Err(ZuulError::NotFound {
                    resource_type: ResourceType::Secret,
                    name: name.to_string(),
                    environment: Some(environment.to_string()),
                });
            }
            Ok(())
        });
        async move { result }
    }

    fn get_metadata(
        &self,
        name: &str,
        environment: &str,
    ) -> impl Future<Output = Result<HashMap<String, String>, ZuulError>> + Send {
        let result = self.check_env_access(environment).map(|()| {
            let state = self.state.lock().unwrap();
            state
                .metadata
                .get(&(name.to_string(), environment.to_string()))
                .cloned()
                .unwrap_or_default()
        });
        async move { result }
    }

    fn set_metadata(
        &self,
        name: &str,
        environment: &str,
        key: &str,
        value: &str,
    ) -> impl Future<Output = Result<(), ZuulError>> + Send {
        let result = self.check_env_access(environment).map(|()| {
            let mut state = self.state.lock().unwrap();
            state
                .metadata
                .entry((name.to_string(), environment.to_string()))
                .or_default()
                .insert(key.to_string(), value.to_string());
        });
        async move { result }
    }

    fn delete_metadata(
        &self,
        name: &str,
        environment: &str,
        key: &str,
    ) -> impl Future<Output = Result<(), ZuulError>> + Send {
        let result = self.check_env_access(environment).map(|()| {
            let mut state = self.state.lock().unwrap();
            if let Some(meta) = state
                .metadata
                .get_mut(&(name.to_string(), environment.to_string()))
            {
                meta.remove(key);
            }
        });
        async move { result }
    }

    fn list_secrets_for_environment(
        &self,
        environment: &str,
    ) -> impl Future<Output = Result<Vec<(String, SecretValue)>, ZuulError>> + Send {
        let result = self.check_env_access(environment).map(|()| {
            let state = self.state.lock().unwrap();
            state
                .secrets
                .iter()
                .filter(|((_, env), _)| env == environment)
                .map(|((name, _), sv)| (name.clone(), sv.clone()))
                .collect()
        });
        async move { result }
    }
}

// ---------------------------------------------------------------------------
// Admin: full access through CLI handlers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_can_create_and_list_environments() {
    let backend = MockBackend::new(AccessLevel::Admin);

    cli::env::create(&backend, "dev", Some("Development"), &OutputFormat::Text)
        .await
        .unwrap();
    cli::env::create(
        &backend,
        "production",
        Some("Production"),
        &OutputFormat::Text,
    )
    .await
    .unwrap();

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
async fn admin_can_delete_environment() {
    let backend = MockBackend::new(AccessLevel::Admin);
    backend.seed_environment("staging", None);

    cli::env::delete(
        &backend,
        "staging",
        true,
        false,
        &OutputFormat::Text,
        PROGRESS,
    )
    .await
    .unwrap();
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
        msg.contains("Permission denied") && msg.contains("IAM"),
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
async fn dev_scoped_cannot_create_env_via_handler() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));

    let err = cli::env::create(&backend, "staging", None, &OutputFormat::Text)
        .await
        .unwrap_err();
    match &err {
        ZuulError::PermissionDenied { resource } => {
            assert!(resource.contains("registry"), "got: {resource}");
        }
        other => panic!("expected PermissionDenied, got: {other}"),
    }
}

#[tokio::test]
async fn dev_scoped_cannot_delete_env_via_handler() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));
    backend.seed_environment("dev", None);

    let err = cli::env::delete(&backend, "dev", true, false, &OutputFormat::Text, PROGRESS)
        .await
        .unwrap_err();
    assert!(matches!(err, ZuulError::PermissionDenied { .. }));
}

#[tokio::test]
async fn dev_scoped_env_list_fails_when_unauthorized_envs_exist() {
    let backend = MockBackend::new(AccessLevel::Scoped(vec!["dev".to_string()]));
    backend.seed_environment("dev", None);
    backend.seed_environment("production", None);

    // env list counts secrets per environment, which triggers PermissionDenied
    // when it reaches an environment the scoped identity can't access.
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

    let err = cli::metadata::set(&backend, "KEY", Some("production"), "owner", "me", true)
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

    // Can read dev and staging
    cli::secret::get(&backend, "KEY", Some("dev"), PROGRESS)
        .await
        .unwrap();
    cli::secret::get(&backend, "KEY", Some("staging"), PROGRESS)
        .await
        .unwrap();

    // Cannot read production
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

#[tokio::test]
async fn unauthenticated_create_env_fails_via_handler() {
    let backend = MockBackend::new(AccessLevel::Unauthenticated);

    let err = cli::env::create(&backend, "dev", None, &OutputFormat::Text)
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
    assert!(msg.contains("IAM"), "should mention IAM: {msg}");
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
