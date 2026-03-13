use std::collections::HashMap;

use chrono::Utc;

use crate::backend::Backend;
use crate::backend::gcp::GcpClient;
use crate::error::{ResourceType, ZuulError};
use crate::models::{Environment, Registry, SecretEntry, SecretValue, validate_environment_name};

/// The GCP secret name used to store the environment registry.
const REGISTRY_SECRET_ID: &str = "zuul__registry";

/// GCP Secret Manager backend implementation.
///
/// Uses [`GcpClient`] for low-level GCP API calls and stores
/// environment metadata in a `zuul__registry` secret as JSON.
pub struct GcpBackend {
    client: GcpClient,
}

impl GcpBackend {
    /// Create a new GCP backend wrapping the given client.
    pub fn new(client: GcpClient) -> Self {
        Self { client }
    }

    /// Read the environment registry from GCP.
    ///
    /// If the registry secret does not exist yet, returns an empty registry.
    async fn read_registry(&self) -> Result<Registry, ZuulError> {
        match self.client.access_secret_version(REGISTRY_SECRET_ID).await {
            Ok(data) => {
                let json = String::from_utf8(data).map_err(|e| {
                    ZuulError::Backend(format!("Registry contains invalid UTF-8: {e}"))
                })?;
                let registry: Registry = serde_json::from_str(&json).map_err(|e| {
                    ZuulError::Backend(format!("Registry contains invalid JSON: {e}"))
                })?;
                Ok(registry)
            }
            Err(ZuulError::Backend(msg)) if msg.contains("not found") => Ok(Registry {
                version: 1,
                environments: HashMap::new(),
            }),
            Err(e) => Err(e),
        }
    }

    /// Write the registry back to GCP.
    ///
    /// Creates the registry secret on first use if it does not exist.
    async fn write_registry(&self, registry: &Registry) -> Result<(), ZuulError> {
        let json = serde_json::to_string_pretty(registry)
            .map_err(|e| ZuulError::Backend(format!("Failed to serialize registry: {e}")))?;

        // Try to add a version. If the secret doesn't exist yet, create it first.
        match self
            .client
            .add_secret_version(REGISTRY_SECRET_ID, json.as_bytes())
            .await
        {
            Ok(()) => Ok(()),
            Err(ZuulError::Backend(msg)) if msg.contains("not found") => {
                self.client
                    .create_secret(REGISTRY_SECRET_ID, HashMap::new(), HashMap::new())
                    .await?;
                self.client
                    .add_secret_version(REGISTRY_SECRET_ID, json.as_bytes())
                    .await
            }
            Err(e) => Err(e),
        }
    }
}

impl Backend for GcpBackend {
    // --- Environment operations ---

    async fn list_environments(&self) -> Result<Vec<Environment>, ZuulError> {
        let registry = self.read_registry().await?;
        let mut envs: Vec<Environment> = registry
            .environments
            .into_iter()
            .map(|(name, mut env)| {
                env.name = name;
                env
            })
            .collect();
        envs.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(envs)
    }

    async fn create_environment(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<Environment, ZuulError> {
        validate_environment_name(name).map_err(ZuulError::Validation)?;

        let mut registry = self.read_registry().await?;

        if registry.environments.contains_key(name) {
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

        registry.environments.insert(name.to_string(), env.clone());
        self.write_registry(&registry).await?;

        Ok(env)
    }

    async fn get_environment(&self, name: &str) -> Result<Environment, ZuulError> {
        let registry = self.read_registry().await?;

        registry
            .environments
            .get(name)
            .cloned()
            .map(|mut env| {
                env.name = name.to_string();
                env
            })
            .ok_or_else(|| ZuulError::NotFound {
                resource_type: ResourceType::Environment,
                name: name.to_string(),
                environment: None,
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

        let mut registry = self.read_registry().await?;

        let mut env = registry
            .environments
            .remove(name)
            .ok_or_else(|| ZuulError::NotFound {
                resource_type: ResourceType::Environment,
                name: name.to_string(),
                environment: None,
            })?;

        let final_name = new_name.unwrap_or(name);

        // If renaming, check the new name isn't already taken.
        if new_name.is_some() && registry.environments.contains_key(final_name) {
            return Err(ZuulError::AlreadyExists {
                resource_type: ResourceType::Environment,
                name: final_name.to_string(),
                environment: None,
            });
        }

        if let Some(desc) = new_description {
            env.description = Some(desc.to_string());
        }
        env.name = final_name.to_string();
        env.updated_at = Utc::now();

        registry
            .environments
            .insert(final_name.to_string(), env.clone());
        self.write_registry(&registry).await?;

        Ok(env)
    }

    async fn delete_environment(&self, name: &str) -> Result<(), ZuulError> {
        let mut registry = self.read_registry().await?;

        if !registry.environments.contains_key(name) {
            return Err(ZuulError::NotFound {
                resource_type: ResourceType::Environment,
                name: name.to_string(),
                environment: None,
            });
        }

        // Delete all secrets bound to this environment.
        let filter = format!("labels.zuul-managed=true AND labels.zuul-env={name}");
        let secrets = self.client.list_secrets(&filter).await?;
        for secret in &secrets {
            // Extract the secret ID from the full resource name
            // (format: "projects/{project}/secrets/{id}")
            if let Some(id) = secret.name.rsplit('/').next() {
                self.client.delete_secret(id).await?;
            }
        }

        registry.environments.remove(name);
        self.write_registry(&registry).await?;

        Ok(())
    }

    // --- Secret operations (implemented in 1.4) ---

    async fn list_secrets(
        &self,
        _environment: Option<&str>,
    ) -> Result<Vec<SecretEntry>, ZuulError> {
        todo!("Implemented in 1.4")
    }

    async fn get_secret(&self, _name: &str, _environment: &str) -> Result<SecretValue, ZuulError> {
        todo!("Implemented in 1.4")
    }

    async fn set_secret(
        &self,
        _name: &str,
        _environment: &str,
        _value: &str,
    ) -> Result<(), ZuulError> {
        todo!("Implemented in 1.4")
    }

    async fn delete_secret(&self, _name: &str, _environment: &str) -> Result<(), ZuulError> {
        todo!("Implemented in 1.4")
    }

    // --- Metadata operations (implemented in 1.5) ---

    async fn get_metadata(
        &self,
        _name: &str,
        _environment: &str,
    ) -> Result<HashMap<String, String>, ZuulError> {
        todo!("Implemented in 1.5")
    }

    async fn set_metadata(
        &self,
        _name: &str,
        _environment: &str,
        _key: &str,
        _value: &str,
    ) -> Result<(), ZuulError> {
        todo!("Implemented in 1.5")
    }

    async fn delete_metadata(
        &self,
        _name: &str,
        _environment: &str,
        _key: &str,
    ) -> Result<(), ZuulError> {
        todo!("Implemented in 1.5")
    }

    // --- Bulk operations (implemented in 1.4) ---

    async fn list_secrets_for_environment(
        &self,
        _environment: &str,
    ) -> Result<Vec<(String, SecretValue)>, ZuulError> {
        todo!("Implemented in 1.4")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a registry with the given environment names.
    fn make_registry(names: &[(&str, Option<&str>)]) -> Registry {
        let now = Utc::now();
        let mut environments = HashMap::new();
        for (name, desc) in names {
            environments.insert(
                name.to_string(),
                Environment {
                    name: String::new(), // populated from map key on read
                    description: desc.map(|d| d.to_string()),
                    created_at: now,
                    updated_at: now,
                },
            );
        }
        Registry {
            version: 1,
            environments,
        }
    }

    #[test]
    fn registry_serialization_roundtrip() {
        let registry = make_registry(&[
            ("dev", Some("Development")),
            ("staging", None),
            ("production", Some("Live")),
        ]);

        let json = serde_json::to_string_pretty(&registry).unwrap();
        let parsed: Registry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.environments.len(), 3);
        assert!(parsed.environments.contains_key("dev"));
        assert!(parsed.environments.contains_key("staging"));
        assert!(parsed.environments.contains_key("production"));
        assert_eq!(
            parsed.environments["dev"].description.as_deref(),
            Some("Development")
        );
        assert_eq!(parsed.environments["staging"].description, None);
    }

    #[test]
    fn empty_registry_serialization() {
        let registry = Registry {
            version: 1,
            environments: HashMap::new(),
        };

        let json = serde_json::to_string(&registry).unwrap();
        let parsed: Registry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert!(parsed.environments.is_empty());
    }

    #[test]
    fn registry_env_name_populated_from_key() {
        let registry = make_registry(&[("dev", Some("Development"))]);
        let json = serde_json::to_string(&registry).unwrap();

        // Environment.name is #[serde(skip)], so it shouldn't appear in JSON
        assert!(!json.contains(r#""name""#));

        let parsed: Registry = serde_json::from_str(&json).unwrap();
        // After deserialization, name field is empty (needs to be set from map key)
        assert_eq!(parsed.environments["dev"].name, "");
    }

    #[test]
    fn registry_spec_format_compatibility() {
        // Verify we can parse the exact JSON format from the spec
        let spec_json = r#"{
            "version": 1,
            "environments": {
                "production": {
                    "description": "Live production environment",
                    "created_at": "2026-03-10T12:00:00Z",
                    "updated_at": "2026-03-10T12:00:00Z"
                },
                "staging": {
                    "description": "Pre-production staging",
                    "created_at": "2026-03-10T12:00:00Z",
                    "updated_at": "2026-03-10T12:00:00Z"
                },
                "dev": {
                    "description": "Local development",
                    "created_at": "2026-03-10T12:00:00Z",
                    "updated_at": "2026-03-10T12:00:00Z"
                }
            }
        }"#;

        let registry: Registry = serde_json::from_str(spec_json).unwrap();
        assert_eq!(registry.version, 1);
        assert_eq!(registry.environments.len(), 3);
        assert_eq!(
            registry.environments["production"].description.as_deref(),
            Some("Live production environment")
        );
    }
}
