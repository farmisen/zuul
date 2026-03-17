//! Shared in-memory mock backend for unit-level tests.
//!
//! Provides a `MockBackend` with configurable IAM-like access rules,
//! usable by any test module that needs a `Backend` without the GCP emulator.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::Utc;

use zuul::backend::Backend;
use zuul::error::{ResourceType, ZuulError};
use zuul::models::{Environment, SecretEntry, SecretValue};

/// Simulated IAM access level for the mock backend.
#[derive(Debug, Clone)]
pub enum AccessLevel {
    Admin,
    Scoped(Vec<String>),
    Unauthenticated,
}

struct MockState {
    environments: HashMap<String, Environment>,
    secrets: HashMap<(String, String), SecretValue>,
    metadata: HashMap<(String, String), HashMap<String, String>>,
}

pub struct MockBackend {
    access: AccessLevel,
    state: Mutex<MockState>,
    /// Environments where metadata write operations should fail (simulates partial failures).
    fail_metadata_envs: Vec<String>,
}

impl MockBackend {
    pub fn new(access: AccessLevel) -> Self {
        Self {
            access,
            state: Mutex::new(MockState {
                environments: HashMap::new(),
                secrets: HashMap::new(),
                metadata: HashMap::new(),
            }),
            fail_metadata_envs: Vec::new(),
        }
    }

    pub fn with_failing_metadata_envs(access: AccessLevel, envs: Vec<String>) -> Self {
        Self {
            access,
            state: Mutex::new(MockState {
                environments: HashMap::new(),
                secrets: HashMap::new(),
                metadata: HashMap::new(),
            }),
            fail_metadata_envs: envs,
        }
    }

    // --- Seeding helpers ---

    pub fn seed_environment(&self, name: &str, description: Option<&str>) {
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

    pub fn seed_secret(&self, name: &str, environment: &str, value: &str) {
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

    pub fn seed_metadata(&self, secret: &str, environment: &str, key: &str, value: &str) {
        self.state
            .lock()
            .unwrap()
            .metadata
            .entry((secret.to_string(), environment.to_string()))
            .or_default()
            .insert(key.to_string(), value.to_string());
    }

    // --- Query helpers ---

    #[allow(dead_code)]
    pub fn has_env(&self, name: &str) -> bool {
        self.state.lock().unwrap().environments.contains_key(name)
    }

    pub fn remove_secret(&self, name: &str, environment: &str) {
        let mut state = self.state.lock().unwrap();
        state
            .secrets
            .remove(&(name.to_string(), environment.to_string()));
    }

    pub fn has_secret(&self, name: &str, environment: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .secrets
            .contains_key(&(name.to_string(), environment.to_string()))
    }

    pub fn get_value(&self, name: &str, environment: &str) -> Option<String> {
        self.state
            .lock()
            .unwrap()
            .secrets
            .get(&(name.to_string(), environment.to_string()))
            .map(|sv| sv.value.clone())
    }

    #[allow(dead_code)]
    pub fn get_meta(&self, secret: &str, environment: &str) -> HashMap<String, String> {
        self.state
            .lock()
            .unwrap()
            .metadata
            .get(&(secret.to_string(), environment.to_string()))
            .cloned()
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn secret_count(&self, environment: &str) -> usize {
        self.state
            .lock()
            .unwrap()
            .secrets
            .keys()
            .filter(|(_, e)| e == environment)
            .count()
    }

    // --- Access control helpers ---

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

    #[allow(dead_code)]
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
            state.metadata.remove(&key);
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
        let result = self.check_env_access(environment).and_then(|()| {
            if self.fail_metadata_envs.iter().any(|e| e == environment) {
                return Err(ZuulError::Backend(format!(
                    "simulated failure for environment '{environment}'"
                )));
            }
            self.seed_metadata(name, environment, key, value);
            Ok(())
        });
        async move { result }
    }

    fn delete_metadata(
        &self,
        name: &str,
        environment: &str,
        key: &str,
    ) -> impl Future<Output = Result<(), ZuulError>> + Send {
        let result = self.check_env_access(environment).and_then(|()| {
            if self.fail_metadata_envs.iter().any(|e| e == environment) {
                return Err(ZuulError::Backend(format!(
                    "simulated failure for environment '{environment}'"
                )));
            }
            let mut state = self.state.lock().unwrap();
            if let Some(meta) = state
                .metadata
                .get_mut(&(name.to_string(), environment.to_string()))
            {
                meta.remove(key);
            }
            Ok(())
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
