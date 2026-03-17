use std::collections::HashMap;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::backend::Backend;
use crate::error::{ResourceType, ZuulError};
use crate::models::{
    Environment, SecretEntry, SecretValue, validate_environment_name, validate_secret_name,
};

/// Default store file name when none is configured.
pub const DEFAULT_STORE_FILE: &str = ".zuul.secrets.enc";

/// Plaintext JSON schema stored inside the encrypted file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Store {
    version: u32,
    environments: HashMap<String, StoredEnvironment>,
    /// Outer key: environment name. Inner key: secret name.
    secrets: HashMap<String, HashMap<String, StoredSecret>>,
    /// Outer key: environment name. Middle key: secret name. Inner: metadata map.
    metadata: HashMap<String, HashMap<String, HashMap<String, String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredEnvironment {
    description: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSecret {
    value: String,
    version: u32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// File-based encrypted backend.
///
/// Stores all environments, secrets, and metadata in a single encrypted
/// JSON file using the `age` encryption format. Designed for local
/// development, small projects, and offline use.
pub struct FileBackend {
    /// Path to the encrypted store file.
    store_path: PathBuf,
    /// Path to an age identity file (if provided).
    #[allow(dead_code)]
    identity: Option<PathBuf>,
}

impl FileBackend {
    /// Create a new file backend with the given store path and optional identity file.
    pub fn new(store_path: PathBuf, identity: Option<PathBuf>) -> Self {
        Self {
            store_path,
            identity,
        }
    }

    /// Resolve the passphrase for encryption/decryption.
    ///
    /// Resolution order:
    /// 1. `ZUUL_PASSPHRASE` env var
    /// 2. Interactive prompt (future — errors for now)
    fn resolve_passphrase(&self) -> Result<age::secrecy::SecretString, ZuulError> {
        if let Ok(passphrase) = std::env::var("ZUUL_PASSPHRASE") {
            return Ok(age::secrecy::SecretString::new(passphrase));
        }

        Err(ZuulError::Auth(
            "No passphrase available. Set ZUUL_PASSPHRASE env var.".to_string(),
        ))
    }

    /// Read and decrypt the store. Returns an empty store if the file doesn't exist.
    fn read_store(&self) -> Result<Store, ZuulError> {
        if !self.store_path.exists() {
            return Ok(Store {
                version: 1,
                ..Default::default()
            });
        }

        let passphrase = self.resolve_passphrase()?;
        let ciphertext = fs::read(&self.store_path).map_err(|e| {
            ZuulError::Backend(format!(
                "Failed to read store '{}': {e}",
                self.store_path.display()
            ))
        })?;

        let decryptor = match age::Decryptor::new(&ciphertext[..])
            .map_err(|e| ZuulError::Backend(format!("Failed to parse encrypted store: {e}")))?
        {
            age::Decryptor::Passphrase(d) => d,
            _ => {
                return Err(ZuulError::Backend(
                    "Store was not encrypted with a passphrase.".to_string(),
                ));
            }
        };

        let mut plaintext = Vec::new();
        decryptor
            .decrypt(&passphrase, None)
            .map_err(|e| ZuulError::Backend(format!("Failed to decrypt store: {e}")))?
            .read_to_end(&mut plaintext)
            .map_err(|e| ZuulError::Backend(format!("Failed to read decrypted data: {e}")))?;

        serde_json::from_slice(&plaintext)
            .map_err(|e| ZuulError::Backend(format!("Failed to parse store JSON: {e}")))
    }

    /// Encrypt and write the store to disk.
    fn write_store(&self, store: &Store) -> Result<(), ZuulError> {
        let plaintext = serde_json::to_vec_pretty(store)
            .map_err(|e| ZuulError::Backend(format!("Failed to serialize store: {e}")))?;

        let passphrase = self.resolve_passphrase()?;
        let encryptor = age::Encryptor::with_user_passphrase(passphrase);

        let mut ciphertext = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut ciphertext)
            .map_err(|e| ZuulError::Backend(format!("Failed to initialize encryption: {e}")))?;
        writer
            .write_all(&plaintext)
            .map_err(|e| ZuulError::Backend(format!("Failed to encrypt: {e}")))?;
        writer
            .finish()
            .map_err(|e| ZuulError::Backend(format!("Failed to finalize encryption: {e}")))?;

        // Write atomically: write to temp file, then rename.
        let tmp_path = self.store_path.with_extension("tmp");
        fs::write(&tmp_path, &ciphertext).map_err(|e| {
            ZuulError::Backend(format!(
                "Failed to write store '{}': {e}",
                tmp_path.display()
            ))
        })?;
        fs::rename(&tmp_path, &self.store_path).map_err(|e| {
            ZuulError::Backend(format!(
                "Failed to finalize store '{}': {e}",
                self.store_path.display()
            ))
        })?;

        Ok(())
    }

    /// Read-modify-write helper with file locking.
    fn with_store<F>(&self, f: F) -> Result<(), ZuulError>
    where
        F: FnOnce(&mut Store) -> Result<(), ZuulError>,
    {
        let _lock = self.lock_store()?;
        let mut store = self.read_store()?;
        f(&mut store)?;
        self.write_store(&store)
    }

    /// Read-modify-write helper that returns a value.
    fn with_store_returning<T, F>(&self, f: F) -> Result<T, ZuulError>
    where
        F: FnOnce(&mut Store) -> Result<T, ZuulError>,
    {
        let _lock = self.lock_store()?;
        let mut store = self.read_store()?;
        let result = f(&mut store)?;
        self.write_store(&store)?;
        Ok(result)
    }

    /// Read-only access with file locking.
    fn with_store_read<T, F>(&self, f: F) -> Result<T, ZuulError>
    where
        F: FnOnce(&Store) -> Result<T, ZuulError>,
    {
        let _lock = self.lock_store()?;
        let store = self.read_store()?;
        f(&store)
    }

    /// Acquire an exclusive lock via a `.lock` sidecar file.
    fn lock_store(&self) -> Result<fs::File, ZuulError> {
        let lock_path = self.store_path.with_extension("lock");
        // Ensure parent directory exists.
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&lock_path)
            .map_err(|e| {
                ZuulError::Backend(format!(
                    "Failed to create lock file '{}': {e}",
                    lock_path.display()
                ))
            })?;
        lock_file
            .lock_exclusive()
            .map_err(|e| ZuulError::Backend(format!("Failed to acquire file lock: {e}")))?;
        Ok(lock_file)
    }
}

impl Backend for FileBackend {
    // --- Environment operations ---

    async fn list_environments(&self) -> Result<Vec<Environment>, ZuulError> {
        self.with_store_read(|store| {
            let mut envs: Vec<Environment> = store
                .environments
                .iter()
                .map(|(name, e)| Environment {
                    name: name.clone(),
                    description: e.description.clone(),
                    created_at: e.created_at,
                    updated_at: e.updated_at,
                })
                .collect();
            envs.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(envs)
        })
    }

    async fn create_environment(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<Environment, ZuulError> {
        validate_environment_name(name).map_err(ZuulError::Validation)?;

        self.with_store_returning(|store| {
            if store.environments.contains_key(name) {
                return Err(ZuulError::AlreadyExists {
                    resource_type: ResourceType::Environment,
                    name: name.to_string(),
                    environment: None,
                });
            }

            let now = Utc::now();
            let stored = StoredEnvironment {
                description: description.map(String::from),
                created_at: now,
                updated_at: now,
            };
            store.environments.insert(name.to_string(), stored.clone());

            Ok(Environment {
                name: name.to_string(),
                description: stored.description,
                created_at: now,
                updated_at: now,
            })
        })
    }

    async fn get_environment(&self, name: &str) -> Result<Environment, ZuulError> {
        validate_environment_name(name).map_err(ZuulError::Validation)?;

        self.with_store_read(|store| {
            store
                .environments
                .get(name)
                .map(|e| Environment {
                    name: name.to_string(),
                    description: e.description.clone(),
                    created_at: e.created_at,
                    updated_at: e.updated_at,
                })
                .ok_or_else(|| ZuulError::NotFound {
                    resource_type: ResourceType::Environment,
                    name: name.to_string(),
                    environment: None,
                })
        })
    }

    async fn update_environment(
        &self,
        name: &str,
        new_name: Option<&str>,
        new_description: Option<&str>,
    ) -> Result<Environment, ZuulError> {
        if let Some(n) = new_name {
            validate_environment_name(n).map_err(ZuulError::Validation)?;
        }

        self.with_store_returning(|store| {
            let mut env = store
                .environments
                .remove(name)
                .ok_or_else(|| ZuulError::NotFound {
                    resource_type: ResourceType::Environment,
                    name: name.to_string(),
                    environment: None,
                })?;

            let final_name = new_name.unwrap_or(name);

            if new_name.is_some() && store.environments.contains_key(final_name) {
                store.environments.insert(name.to_string(), env);
                return Err(ZuulError::AlreadyExists {
                    resource_type: ResourceType::Environment,
                    name: final_name.to_string(),
                    environment: None,
                });
            }

            if let Some(desc) = new_description {
                env.description = Some(desc.to_string());
            }
            env.updated_at = Utc::now();

            if new_name.is_some() && name != final_name {
                if let Some(secrets) = store.secrets.remove(name) {
                    store.secrets.insert(final_name.to_string(), secrets);
                }
                if let Some(metadata) = store.metadata.remove(name) {
                    store.metadata.insert(final_name.to_string(), metadata);
                }
            }

            store
                .environments
                .insert(final_name.to_string(), env.clone());

            Ok(Environment {
                name: final_name.to_string(),
                description: env.description,
                created_at: env.created_at,
                updated_at: env.updated_at,
            })
        })
    }

    async fn delete_environment(&self, name: &str) -> Result<(), ZuulError> {
        validate_environment_name(name).map_err(ZuulError::Validation)?;

        self.with_store(|store| {
            if !store.environments.contains_key(name) {
                return Err(ZuulError::NotFound {
                    resource_type: ResourceType::Environment,
                    name: name.to_string(),
                    environment: None,
                });
            }

            store.environments.remove(name);
            store.secrets.remove(name);
            store.metadata.remove(name);
            Ok(())
        })
    }

    // --- Secret operations ---

    async fn list_secrets(&self, environment: Option<&str>) -> Result<Vec<SecretEntry>, ZuulError> {
        self.with_store_read(|store| {
            let mut entries: HashMap<String, Vec<String>> = HashMap::new();

            for (env_name, env_secrets) in &store.secrets {
                if let Some(filter) = environment
                    && env_name != filter
                {
                    continue;
                }
                for secret_name in env_secrets.keys() {
                    entries
                        .entry(secret_name.clone())
                        .or_default()
                        .push(env_name.clone());
                }
            }

            let mut result: Vec<SecretEntry> = entries
                .into_iter()
                .map(|(name, mut environments)| {
                    environments.sort();
                    SecretEntry {
                        name,
                        environments,
                        metadata: HashMap::new(),
                    }
                })
                .collect();
            result.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(result)
        })
    }

    async fn get_secret(&self, name: &str, environment: &str) -> Result<SecretValue, ZuulError> {
        self.with_store_read(|store| {
            store
                .secrets
                .get(environment)
                .and_then(|env| env.get(name))
                .map(|s| SecretValue {
                    name: name.to_string(),
                    environment: environment.to_string(),
                    value: s.value.clone(),
                    version: s.version.to_string(),
                    created_at: s.created_at,
                    updated_at: s.updated_at,
                })
                .ok_or_else(|| ZuulError::NotFound {
                    resource_type: ResourceType::Secret,
                    name: name.to_string(),
                    environment: Some(environment.to_string()),
                })
        })
    }

    async fn set_secret(
        &self,
        name: &str,
        environment: &str,
        value: &str,
    ) -> Result<(), ZuulError> {
        validate_secret_name(name).map_err(ZuulError::Validation)?;

        self.with_store(|store| {
            if !store.environments.contains_key(environment) {
                return Err(ZuulError::NotFound {
                    resource_type: ResourceType::Environment,
                    name: environment.to_string(),
                    environment: None,
                });
            }

            let env_secrets = store.secrets.entry(environment.to_string()).or_default();
            let now = Utc::now();

            if let Some(existing) = env_secrets.get_mut(name) {
                existing.value = value.to_string();
                existing.version += 1;
                existing.updated_at = now;
            } else {
                env_secrets.insert(
                    name.to_string(),
                    StoredSecret {
                        value: value.to_string(),
                        version: 1,
                        created_at: now,
                        updated_at: now,
                    },
                );
            }

            Ok(())
        })
    }

    async fn delete_secret(&self, name: &str, environment: &str) -> Result<(), ZuulError> {
        self.with_store(|store| {
            let env_secrets =
                store
                    .secrets
                    .get_mut(environment)
                    .ok_or_else(|| ZuulError::NotFound {
                        resource_type: ResourceType::Secret,
                        name: name.to_string(),
                        environment: Some(environment.to_string()),
                    })?;

            if env_secrets.remove(name).is_none() {
                return Err(ZuulError::NotFound {
                    resource_type: ResourceType::Secret,
                    name: name.to_string(),
                    environment: Some(environment.to_string()),
                });
            }

            if let Some(env_meta) = store.metadata.get_mut(environment) {
                env_meta.remove(name);
            }

            Ok(())
        })
    }

    // --- Metadata operations ---

    async fn get_metadata(
        &self,
        name: &str,
        environment: &str,
    ) -> Result<HashMap<String, String>, ZuulError> {
        self.with_store_read(|store| {
            let exists = store
                .secrets
                .get(environment)
                .is_some_and(|env| env.contains_key(name));
            if !exists {
                return Err(ZuulError::NotFound {
                    resource_type: ResourceType::Secret,
                    name: name.to_string(),
                    environment: Some(environment.to_string()),
                });
            }

            Ok(store
                .metadata
                .get(environment)
                .and_then(|env| env.get(name))
                .cloned()
                .unwrap_or_default())
        })
    }

    async fn set_metadata(
        &self,
        name: &str,
        environment: &str,
        key: &str,
        value: &str,
    ) -> Result<(), ZuulError> {
        self.with_store(|store| {
            let exists = store
                .secrets
                .get(environment)
                .is_some_and(|env| env.contains_key(name));
            if !exists {
                return Err(ZuulError::NotFound {
                    resource_type: ResourceType::Secret,
                    name: name.to_string(),
                    environment: Some(environment.to_string()),
                });
            }

            store
                .metadata
                .entry(environment.to_string())
                .or_default()
                .entry(name.to_string())
                .or_default()
                .insert(key.to_string(), value.to_string());

            Ok(())
        })
    }

    async fn delete_metadata(
        &self,
        name: &str,
        environment: &str,
        key: &str,
    ) -> Result<(), ZuulError> {
        self.with_store(|store| {
            if let Some(env_meta) = store.metadata.get_mut(environment)
                && let Some(secret_meta) = env_meta.get_mut(name)
            {
                secret_meta.remove(key);
            }
            Ok(())
        })
    }

    // --- Bulk operations ---

    async fn list_secrets_for_environment(
        &self,
        environment: &str,
    ) -> Result<Vec<(String, SecretValue)>, ZuulError> {
        self.with_store_read(|store| {
            let env_secrets = match store.secrets.get(environment) {
                Some(s) => s,
                None => return Ok(Vec::new()),
            };

            let mut result: Vec<(String, SecretValue)> = env_secrets
                .iter()
                .map(|(name, s)| {
                    (
                        name.clone(),
                        SecretValue {
                            name: name.clone(),
                            environment: environment.to_string(),
                            value: s.value.clone(),
                            version: s.version.to_string(),
                            created_at: s.created_at,
                            updated_at: s.updated_at,
                        },
                    )
                })
                .collect();
            result.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(result)
        })
    }
}
