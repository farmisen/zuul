use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::backend::Backend;
use crate::backend::gcp::GcpClient;
use crate::error::{ResourceType, ZuulError};
use crate::models::{
    Environment, METADATA_PREFIX, Registry, SecretEntry, SecretValue, validate_environment_name,
    validate_metadata_key, validate_secret_name,
};

/// The GCP secret name used to store the environment registry.
const REGISTRY_SECRET_ID: &str = "zuul__registry";

/// Map a backend "not found" error to a typed `ZuulError::NotFound` for secrets.
fn map_secret_not_found(name: &str, environment: &str, err: ZuulError) -> ZuulError {
    if matches!(&err, ZuulError::Backend(msg) if msg.contains("not found")) {
        ZuulError::NotFound {
            resource_type: ResourceType::Secret,
            name: name.to_string(),
            environment: Some(environment.to_string()),
        }
    } else {
        err
    }
}

/// Convert a `google_cloud_wkt::Timestamp` to a chrono `DateTime<Utc>`.
fn wkt_timestamp_to_chrono(ts: Option<google_cloud_wkt::Timestamp>) -> DateTime<Utc> {
    ts.and_then(|t| DateTime::from_timestamp(t.seconds(), t.nanos() as u32))
        .unwrap_or_else(Utc::now)
}

/// GCP Secret Manager backend implementation.
///
/// Uses [`GcpClient`] for low-level GCP API calls and stores
/// environment metadata in a `zuul__registry` secret as JSON.
pub struct GcpBackend {
    client: GcpClient,
    /// Resolved credentials file path (for gcloud CLI commands like audit).
    credentials_path: Option<String>,
}

impl GcpBackend {
    /// Create a new GCP backend wrapping the given client.
    pub fn new(client: GcpClient, credentials_path: Option<String>) -> Self {
        Self {
            client,
            credentials_path,
        }
    }

    /// Build the GCP secret ID for a zuul-managed secret.
    fn secret_id(environment: &str, name: &str) -> String {
        format!("zuul__{environment}__{name}")
    }

    /// Parse a GCP secret resource name or ID into `(env, name)`.
    ///
    /// Handles both full resource names (`projects/p/secrets/zuul__env__NAME`)
    /// and bare IDs (`zuul__env__NAME`). Returns `None` for non-zuul secrets
    /// or the registry secret.
    fn parse_secret_id(resource_name: &str) -> Option<(&str, &str)> {
        let secret_id = resource_name.rsplit('/').next().unwrap_or(resource_name);
        let rest = secret_id.strip_prefix("zuul__")?;
        rest.split_once("__")
    }

    /// Build the standard labels for a zuul-managed secret.
    fn zuul_labels(environment: &str, name: &str) -> HashMap<String, String> {
        HashMap::from([
            ("zuul-managed".to_string(), "true".to_string()),
            ("zuul-env".to_string(), environment.to_lowercase()),
            ("zuul-name".to_string(), name.to_lowercase()),
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
}

impl Backend for GcpBackend {
    // --- Environment operations ---

    async fn create_environment(
        &self,
        _name: &str,
        _description: Option<&str>,
    ) -> Result<Environment, ZuulError> {
        Err(ZuulError::Config(
            "Environment management is handled by Terraform for the GCP backend. \
             Run `terraform apply` to create environments. \
             See docs/gcp-env-playbook.md for details."
                .to_string(),
        ))
    }

    async fn update_environment(
        &self,
        _name: &str,
        _new_name: Option<&str>,
        _new_description: Option<&str>,
    ) -> Result<Environment, ZuulError> {
        Err(ZuulError::Config(
            "Environment management is handled by Terraform for the GCP backend. \
             Run `terraform apply` to update environments. \
             See docs/gcp-env-playbook.md for details."
                .to_string(),
        ))
    }

    async fn delete_environment(&self, _name: &str) -> Result<(), ZuulError> {
        Err(ZuulError::Config(
            "Environment management is handled by Terraform for the GCP backend. \
             Run `terraform apply` to delete environments. \
             See docs/gcp-env-playbook.md for details."
                .to_string(),
        ))
    }

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

    async fn get_environment(&self, name: &str) -> Result<Environment, ZuulError> {
        validate_environment_name(name).map_err(ZuulError::Validation)?;
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

    // --- Secret operations ---

    async fn list_secrets(&self, environment: Option<&str>) -> Result<Vec<SecretEntry>, ZuulError> {
        if let Some(env) = environment {
            validate_environment_name(env).map_err(ZuulError::Validation)?;
        }
        let filter = match environment {
            Some(env) => format!("labels.zuul-managed=true AND labels.zuul-env={env}"),
            None => "labels.zuul-managed=true".to_string(),
        };

        let secrets = self.client.list_secrets(&filter).await?;

        // Group by secret name, collecting environments.
        // Extract name and env from the secret ID (zuul__{env}__{name}) rather than
        // labels, since labels are lowercased to satisfy GCP constraints.
        let mut entries: HashMap<String, Vec<String>> = HashMap::new();
        for secret in &secrets {
            if let Some((env, name)) = Self::parse_secret_id(&secret.name) {
                entries
                    .entry(name.to_string())
                    .or_default()
                    .push(env.to_string());
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
        validate_secret_name(name).map_err(ZuulError::Validation)?;
        self.ensure_environment_exists(environment).await?;

        let secret_id = Self::secret_id(environment, name);

        // Get the secret metadata for create_time.
        let secret_meta = self
            .client
            .get_secret(&secret_id)
            .await
            .map_err(|e| map_secret_not_found(name, environment, e))?;

        let created_at = wkt_timestamp_to_chrono(secret_meta.create_time);

        // Access the latest version for the value and version name.
        let (data, version_name) = self
            .client
            .access_secret_version(&secret_id)
            .await
            .map_err(|e| map_secret_not_found(name, environment, e))?;

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
        validate_secret_name(name).map_err(ZuulError::Validation)?;
        self.ensure_environment_exists(environment).await?;

        let secret_id = Self::secret_id(environment, name);

        self.client
            .delete_secret(&secret_id)
            .await
            .map_err(|e| map_secret_not_found(name, environment, e))
    }

    // --- Metadata operations ---

    async fn get_metadata(
        &self,
        name: &str,
        environment: &str,
    ) -> Result<HashMap<String, String>, ZuulError> {
        validate_secret_name(name).map_err(ZuulError::Validation)?;
        self.ensure_environment_exists(environment).await?;

        let secret_id = Self::secret_id(environment, name);
        let secret = self
            .client
            .get_secret(&secret_id)
            .await
            .map_err(|e| map_secret_not_found(name, environment, e))?;

        let metadata = secret
            .annotations
            .into_iter()
            .filter_map(|(k, v)| k.strip_prefix(METADATA_PREFIX).map(|k| (k.to_string(), v)))
            .collect();

        Ok(metadata)
    }

    async fn set_metadata(
        &self,
        name: &str,
        environment: &str,
        key: &str,
        value: &str,
    ) -> Result<(), ZuulError> {
        validate_metadata_key(key).map_err(ZuulError::Validation)?;
        self.ensure_environment_exists(environment).await?;

        let secret_id = Self::secret_id(environment, name);
        let secret = self
            .client
            .get_secret(&secret_id)
            .await
            .map_err(|e| map_secret_not_found(name, environment, e))?;

        let mut annotations = secret.annotations;
        annotations.insert(format!("{METADATA_PREFIX}{key}"), value.to_string());

        self.client
            .update_secret(&secret_id, None, Some(annotations))
            .await?;

        Ok(())
    }

    async fn delete_metadata(
        &self,
        name: &str,
        environment: &str,
        key: &str,
    ) -> Result<(), ZuulError> {
        validate_metadata_key(key).map_err(ZuulError::Validation)?;
        self.ensure_environment_exists(environment).await?;

        let secret_id = Self::secret_id(environment, name);
        let secret = self
            .client
            .get_secret(&secret_id)
            .await
            .map_err(|e| map_secret_not_found(name, environment, e))?;

        let annotation_key = format!("{METADATA_PREFIX}{key}");
        let mut annotations = secret.annotations;
        if annotations.remove(&annotation_key).is_none() {
            return Err(ZuulError::NotFound {
                resource_type: ResourceType::Secret,
                name: format!("metadata key '{key}' on secret '{name}'"),
                environment: Some(environment.to_string()),
            });
        }

        self.client
            .update_secret(&secret_id, None, Some(annotations))
            .await?;

        Ok(())
    }

    // --- Bulk operations ---

    async fn list_secrets_for_environment(
        &self,
        environment: &str,
    ) -> Result<Vec<(String, SecretValue)>, ZuulError> {
        use futures::stream::{self, StreamExt};

        /// Max concurrent `AccessSecretVersion` requests.
        /// GCP allows 90,000/min (1,500/sec); 20 is conservative.
        const MAX_CONCURRENT: usize = 20;

        let entries = self.list_secrets(Some(environment)).await?;
        let env = environment.to_string();

        let results: Vec<_> = stream::iter(entries.into_iter().map(|entry| {
            let env = env.clone();
            async move {
                match self.get_secret(&entry.name, &env).await {
                    Ok(secret_value) => Some((entry.name, secret_value)),
                    Err(_) => None,
                }
            }
        }))
        .buffer_unordered(MAX_CONCURRENT)
        .filter_map(|r| async { r })
        .collect()
        .await;

        Ok(results)
    }

    async fn audit_access(&self) -> Result<Vec<crate::models::AccessBinding>, ZuulError> {
        let mut args = vec![
            "projects".to_string(),
            "get-iam-policy".to_string(),
            self.client.project_id.clone(),
            "--format=json".to_string(),
        ];

        // Use the configured SA key so gcloud authenticates as the right identity,
        // regardless of which gcloud account is currently active.
        if let Some(ref creds) = self.credentials_path {
            let expanded = crate::config::expand_tilde(creds);
            args.push(format!("--credential-file-override={expanded}"));
        }

        let output = std::process::Command::new("gcloud")
            .args(&args)
            .output()
            .map_err(|e| {
                ZuulError::Backend(format!(
                    "Failed to run 'gcloud projects get-iam-policy': {e}. Is the gcloud CLI installed?"
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZuulError::Backend(format!(
                "gcloud get-iam-policy failed: {stderr}"
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let policy: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| ZuulError::Backend(format!("Failed to parse IAM policy: {e}")))?;

        parse_iam_policy(&policy, &self.client.project_id)
    }
}

/// Map a GCP IAM role to a zuul access level.
fn map_role(role: &str) -> Option<&str> {
    match role {
        "roles/secretmanager.admin" => Some("admin"),
        "roles/secretmanager.secretAccessor" => Some("read"),
        "roles/secretmanager.secretVersionManager" => Some("write"),
        _ => None,
    }
}

/// Extract the zuul environment name from an IAM condition expression.
fn extract_env_from_condition(expression: &str, project_id: &str) -> Option<String> {
    let prefix = format!("resource.name.startsWith(\"projects/{project_id}/secrets/zuul__");
    if let Some(rest) = expression.strip_prefix(&prefix)
        && let Some(env) = rest.strip_suffix("__\")")
    {
        return Some(env.to_string());
    }
    None
}

/// Parse a GCP IAM policy JSON into zuul AccessBindings.
fn parse_iam_policy(
    policy: &serde_json::Value,
    project_id: &str,
) -> Result<Vec<crate::models::AccessBinding>, ZuulError> {
    let bindings = policy
        .get("bindings")
        .and_then(|b| b.as_array())
        .unwrap_or(&Vec::new())
        .clone();

    let mut result = Vec::new();

    for binding in &bindings {
        let role = binding.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let zuul_role = match map_role(role) {
            Some(r) => r,
            None => continue,
        };

        let members = binding
            .get("members")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();

        let condition_expr = binding
            .get("condition")
            .and_then(|c| c.get("expression"))
            .and_then(|e| e.as_str());

        let environment =
            condition_expr.and_then(|expr| extract_env_from_condition(expr, project_id));

        // Skip bindings with conditions that don't match an environment
        // (e.g., registry reader bindings). Only include bindings that are
        // either unconditional (project-wide) or match a zuul environment.
        if condition_expr.is_some() && environment.is_none() {
            continue;
        }

        for member in &members {
            if let Some(identity) = member.as_str() {
                result.push(crate::models::AccessBinding {
                    identity: identity.to_string(),
                    environment: environment.clone(),
                    role: zuul_role.to_string(),
                });
            }
        }
    }

    result.sort_by(|a, b| {
        a.identity
            .cmp(&b.identity)
            .then(a.environment.cmp(&b.environment))
    });
    Ok(result)
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
    fn parse_secret_id_full_resource_name() {
        assert_eq!(
            GcpBackend::parse_secret_id("projects/my-proj/secrets/zuul__production__DATABASE_URL"),
            Some(("production", "DATABASE_URL"))
        );
    }

    #[test]
    fn parse_secret_id_bare_id() {
        assert_eq!(
            GcpBackend::parse_secret_id("zuul__dev__API_KEY"),
            Some(("dev", "API_KEY"))
        );
    }

    #[test]
    fn parse_secret_id_registry() {
        assert_eq!(GcpBackend::parse_secret_id("zuul__registry"), None);
    }

    #[test]
    fn parse_secret_id_non_zuul() {
        assert_eq!(
            GcpBackend::parse_secret_id("projects/p/secrets/other-secret"),
            None
        );
    }

    #[test]
    fn zuul_labels_contains_required_keys() {
        let labels = GcpBackend::zuul_labels("staging", "SECRET_KEY");
        assert_eq!(labels.get("zuul-managed").unwrap(), "true");
        assert_eq!(labels.get("zuul-env").unwrap(), "staging");
        assert_eq!(labels.get("zuul-name").unwrap(), "secret_key");
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
    fn wkt_timestamp_conversion() {
        let ts = google_cloud_wkt::Timestamp::clamp(1710072000, 0); // 2024-03-10T12:00:00Z
        let dt = wkt_timestamp_to_chrono(Some(ts));
        assert_eq!(dt.timestamp(), 1710072000);
    }

    #[test]
    fn wkt_timestamp_none_returns_now() {
        let dt = wkt_timestamp_to_chrono(None);
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

    #[test]
    fn map_role_admin() {
        assert_eq!(map_role("roles/secretmanager.admin"), Some("admin"));
    }

    #[test]
    fn map_role_accessor() {
        assert_eq!(map_role("roles/secretmanager.secretAccessor"), Some("read"));
    }

    #[test]
    fn map_role_writer() {
        assert_eq!(
            map_role("roles/secretmanager.secretVersionManager"),
            Some("write")
        );
    }

    #[test]
    fn map_role_unknown() {
        assert_eq!(map_role("roles/viewer"), None);
    }

    #[test]
    fn extract_env_valid() {
        let expr = r#"resource.name.startsWith("projects/my-proj/secrets/zuul__dev__")"#;
        assert_eq!(
            extract_env_from_condition(expr, "my-proj"),
            Some("dev".to_string())
        );
    }

    #[test]
    fn extract_env_no_match() {
        let expr = r#"resource.name == "projects/my-proj/secrets/zuul__registry""#;
        assert_eq!(extract_env_from_condition(expr, "my-proj"), None);
    }

    #[test]
    fn parse_iam_policy_basic() {
        let policy = serde_json::json!({
            "bindings": [
                {
                    "role": "roles/secretmanager.admin",
                    "members": ["user:admin@co.com"]
                },
                {
                    "role": "roles/secretmanager.secretAccessor",
                    "members": ["user:dev@co.com"],
                    "condition": {
                        "expression": "resource.name.startsWith(\"projects/my-proj/secrets/zuul__dev__\")"
                    }
                }
            ]
        });
        let bindings = parse_iam_policy(&policy, "my-proj").unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].identity, "user:admin@co.com");
        assert_eq!(bindings[0].role, "admin");
        assert!(bindings[0].environment.is_none());
        assert_eq!(bindings[1].identity, "user:dev@co.com");
        assert_eq!(bindings[1].role, "read");
        assert_eq!(bindings[1].environment.as_deref(), Some("dev"));
    }

    #[test]
    fn parse_iam_policy_skips_non_secretmanager_roles() {
        let policy = serde_json::json!({
            "bindings": [
                {
                    "role": "roles/viewer",
                    "members": ["user:someone@co.com"]
                }
            ]
        });
        let bindings = parse_iam_policy(&policy, "my-proj").unwrap();
        assert!(bindings.is_empty());
    }
}
