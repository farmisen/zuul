use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::backend::Backend;
use crate::backend::gcp::GcpClient;
use crate::error::{ResourceType, ZuulError};
use crate::models::{
    Environment, Registry, SecretEntry, SecretValue, validate_environment_name,
    validate_secret_name,
};

/// The GCP secret name used to store the environment registry.
const REGISTRY_SECRET_ID: &str = "zuul__registry";

/// Convert a protobuf `Timestamp` to a chrono `DateTime<Utc>`.
fn proto_timestamp_to_chrono(ts: Option<gcloud_sdk::prost_types::Timestamp>) -> DateTime<Utc> {
    ts.and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos as u32))
        .unwrap_or_else(Utc::now)
}

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

    /// Build the GCP secret ID for a zuul-managed secret.
    fn secret_id(environment: &str, name: &str) -> String {
        format!("zuul__{environment}__{name}")
    }

    /// Build the standard labels for a zuul-managed secret.
    fn zuul_labels(environment: &str, name: &str) -> HashMap<String, String> {
        HashMap::from([
            ("zuul-managed".to_string(), "true".to_string()),
            ("zuul-env".to_string(), environment.to_string()),
            ("zuul-name".to_string(), name.to_string()),
        ])
    }

    /// Extract the version number from a full version resource name.
    ///
    /// Input format: `projects/{project}/secrets/{id}/versions/{version}`
    fn extract_version(version_name: &str) -> String {
        version_name
            .rsplit('/')
            .next()
            .unwrap_or("unknown")
            .to_string()
    }

    /// Verify that an environment exists in the registry.
    async fn ensure_environment_exists(&self, environment: &str) -> Result<(), ZuulError> {
        let registry = self.read_registry().await?;
        if !registry.environments.contains_key(environment) {
            return Err(ZuulError::NotFound {
                resource_type: ResourceType::Environment,
                name: environment.to_string(),
                environment: None,
            });
        }
        Ok(())
    }

    /// Read the environment registry from GCP.
    ///
    /// If the registry secret does not exist yet, returns an empty registry.
    async fn read_registry(&self) -> Result<Registry, ZuulError> {
        match self.client.access_secret_version(REGISTRY_SECRET_ID).await {
            Ok((data, _)) => {
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

    // --- Secret operations ---

    async fn list_secrets(&self, environment: Option<&str>) -> Result<Vec<SecretEntry>, ZuulError> {
        let filter = match environment {
            Some(env) => format!("labels.zuul-managed=true AND labels.zuul-env={env}"),
            None => "labels.zuul-managed=true".to_string(),
        };

        let secrets = self.client.list_secrets(&filter).await?;

        // Group by secret name, collecting environments.
        let mut entries: HashMap<String, Vec<String>> = HashMap::new();
        for secret in &secrets {
            let name = secret.labels.get("zuul-name").cloned().unwrap_or_default();
            let env = secret.labels.get("zuul-env").cloned().unwrap_or_default();
            if !name.is_empty() {
                entries.entry(name).or_default().push(env);
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
    }

    async fn get_secret(&self, name: &str, environment: &str) -> Result<SecretValue, ZuulError> {
        self.ensure_environment_exists(environment).await?;

        let secret_id = Self::secret_id(environment, name);

        // Get the secret metadata for create_time.
        let secret_meta = self.client.get_secret(&secret_id).await.map_err(|e| {
            if matches!(&e, ZuulError::Backend(msg) if msg.contains("not found")) {
                ZuulError::NotFound {
                    resource_type: ResourceType::Secret,
                    name: name.to_string(),
                    environment: Some(environment.to_string()),
                }
            } else {
                e
            }
        })?;

        let created_at = proto_timestamp_to_chrono(secret_meta.create_time);

        // Access the latest version for the value and version name.
        let (data, version_name) = self
            .client
            .access_secret_version(&secret_id)
            .await
            .map_err(|e| {
                if matches!(&e, ZuulError::Backend(msg) if msg.contains("not found")) {
                    ZuulError::NotFound {
                        resource_type: ResourceType::Secret,
                        name: name.to_string(),
                        environment: Some(environment.to_string()),
                    }
                } else {
                    e
                }
            })?;

        let value = String::from_utf8(data)
            .map_err(|e| ZuulError::Backend(format!("Secret value contains invalid UTF-8: {e}")))?;

        Ok(SecretValue {
            name: name.to_string(),
            environment: environment.to_string(),
            value,
            version: Self::extract_version(&version_name),
            created_at,
            updated_at: created_at,
        })
    }

    async fn set_secret(
        &self,
        name: &str,
        environment: &str,
        value: &str,
    ) -> Result<(), ZuulError> {
        validate_secret_name(name).map_err(ZuulError::Validation)?;
        self.ensure_environment_exists(environment).await?;

        let secret_id = Self::secret_id(environment, name);
        let labels = Self::zuul_labels(environment, name);

        // Try to add a version. If the secret doesn't exist, create it first.
        match self
            .client
            .add_secret_version(&secret_id, value.as_bytes())
            .await
        {
            Ok(()) => Ok(()),
            Err(ZuulError::Backend(msg)) if msg.contains("not found") => {
                self.client
                    .create_secret(&secret_id, labels, HashMap::new())
                    .await?;
                self.client
                    .add_secret_version(&secret_id, value.as_bytes())
                    .await
            }
            Err(e) => Err(e),
        }
    }

    async fn delete_secret(&self, name: &str, environment: &str) -> Result<(), ZuulError> {
        self.ensure_environment_exists(environment).await?;

        let secret_id = Self::secret_id(environment, name);

        self.client.delete_secret(&secret_id).await.map_err(|e| {
            if matches!(&e, ZuulError::Backend(msg) if msg.contains("not found")) {
                ZuulError::NotFound {
                    resource_type: ResourceType::Secret,
                    name: name.to_string(),
                    environment: Some(environment.to_string()),
                }
            } else {
                e
            }
        })
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

    // --- Bulk operations ---

    async fn list_secrets_for_environment(
        &self,
        environment: &str,
    ) -> Result<Vec<(String, SecretValue)>, ZuulError> {
        self.ensure_environment_exists(environment).await?;

        let filter = format!("labels.zuul-managed=true AND labels.zuul-env={environment}");
        let secrets = self.client.list_secrets(&filter).await?;

        let mut results = Vec::new();
        for secret in &secrets {
            let name = match secret.labels.get("zuul-name") {
                Some(n) => n.clone(),
                None => continue,
            };

            let gcp_secret_id = match secret.name.rsplit('/').next() {
                Some(id) => id,
                None => continue,
            };

            let created_at = proto_timestamp_to_chrono(secret.create_time);

            match self.client.access_secret_version(gcp_secret_id).await {
                Ok((data, version_name)) => {
                    let value = String::from_utf8(data).unwrap_or_default();
                    results.push((
                        name.clone(),
                        SecretValue {
                            name,
                            environment: environment.to_string(),
                            value,
                            version: Self::extract_version(&version_name),
                            created_at,
                            updated_at: created_at,
                        },
                    ));
                }
                Err(_) => continue,
            }
        }

        results.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(results)
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

    // --- Helper function tests ---

    #[test]
    fn secret_id_format() {
        assert_eq!(
            GcpBackend::secret_id("production", "DATABASE_URL"),
            "zuul__production__DATABASE_URL"
        );
        assert_eq!(
            GcpBackend::secret_id("dev", "API_KEY"),
            "zuul__dev__API_KEY"
        );
    }

    #[test]
    fn zuul_labels_contains_required_keys() {
        let labels = GcpBackend::zuul_labels("staging", "SECRET_KEY");
        assert_eq!(labels.get("zuul-managed").unwrap(), "true");
        assert_eq!(labels.get("zuul-env").unwrap(), "staging");
        assert_eq!(labels.get("zuul-name").unwrap(), "SECRET_KEY");
        assert_eq!(labels.len(), 3);
    }

    #[test]
    fn extract_version_from_resource_name() {
        assert_eq!(
            GcpBackend::extract_version("projects/my-proj/secrets/my-secret/versions/3"),
            "3"
        );
        assert_eq!(
            GcpBackend::extract_version("projects/p/secrets/s/versions/42"),
            "42"
        );
    }

    #[test]
    fn extract_version_from_bare_number() {
        assert_eq!(GcpBackend::extract_version("7"), "7");
    }

    #[test]
    fn extract_version_empty_string() {
        assert_eq!(GcpBackend::extract_version(""), "");
    }

    #[test]
    fn proto_timestamp_conversion() {
        let ts = gcloud_sdk::prost_types::Timestamp {
            seconds: 1710072000, // 2024-03-10T12:00:00Z
            nanos: 0,
        };
        let dt = proto_timestamp_to_chrono(Some(ts));
        assert_eq!(dt.timestamp(), 1710072000);
    }

    #[test]
    fn proto_timestamp_none_returns_now() {
        let dt = proto_timestamp_to_chrono(None);
        // Should be very close to now
        let diff = (Utc::now() - dt).num_seconds().abs();
        assert!(diff < 2);
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
