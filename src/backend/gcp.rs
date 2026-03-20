use std::collections::HashMap;
use std::env;

use google_cloud_gax::error::rpc::Code;
use google_cloud_gax::paginator::ItemPaginator;
use google_cloud_secretmanager_v1::client::SecretManagerService;
use google_cloud_secretmanager_v1::model::{
    Replication, Secret, SecretPayload, replication::Automatic,
};

use crate::error::ZuulError;

/// Environment variable to override the Secret Manager endpoint (for emulator use).
const EMULATOR_HOST_ENV: &str = "SECRET_MANAGER_EMULATOR_HOST";

/// Resolve a raw credentials string to a parsed JSON value.
///
/// Resolution order:
/// 1. If the value is a path to an existing file, read and parse it.
/// 2. Otherwise, try to parse it as inline JSON directly.
/// 3. If neither, return an error.
fn resolve_credentials(raw: &str) -> Result<serde_json::Value, ZuulError> {
    let trimmed = raw.trim();

    // 1. Check if it's an existing file path.
    if std::path::Path::new(trimmed).is_file() {
        let content = std::fs::read_to_string(trimmed).map_err(|e| {
            ZuulError::Config(format!("Failed to read credentials file '{trimmed}': {e}"))
        })?;
        return serde_json::from_str(&content).map_err(|e| {
            ZuulError::Config(format!(
                "Credentials file '{trimmed}' contains invalid JSON: {e}"
            ))
        });
    }

    // 2. Try to parse as inline JSON.
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(value);
    }

    // 3. Neither a file nor valid JSON.
    Err(ZuulError::Config(format!(
        "ZUUL_GCP_CREDENTIALS is not a valid file path (file not found) \
         and not valid JSON: '{}'",
        if trimmed.len() > 40 {
            format!("{}...", &trimmed[..40])
        } else {
            trimmed.to_string()
        }
    )))
}

/// GCP Secret Manager client wrapper.
///
/// Handles authentication via Application Default Credentials (ADC),
/// a service account key file, or inline service account JSON. Retry
/// and pagination are managed by the underlying official SDK.
pub struct GcpClient {
    client: SecretManagerService,
    pub(crate) project_id: String,
}

impl GcpClient {
    /// Create a new GCP Secret Manager client.
    ///
    /// Authentication is resolved in this order:
    /// 1. If `SECRET_MANAGER_EMULATOR_HOST` is set, connects to that endpoint
    ///    with a static dummy token (no real credentials needed).
    /// 2. `credentials` argument (from config or CLI) — file path or inline JSON.
    /// 3. Application Default Credentials (ADC) — e.g., from `gcloud auth application-default login`.
    pub async fn new(project_id: &str, credentials: Option<&str>) -> Result<Self, ZuulError> {
        let client = if let Ok(emulator_host) = env::var(EMULATOR_HOST_ENV) {
            // Connect to emulator with a static dummy token.
            let creds = google_cloud_auth::credentials::anonymous::Builder::new().build();

            SecretManagerService::builder()
                .with_endpoint(&emulator_host)
                .with_credentials(creds)
                .build()
                .await
        } else {
            // Resolve credentials: explicit path/JSON > ZUUL_GCP_CREDENTIALS env > ADC.
            let raw = credentials
                .map(String::from)
                .or_else(|| env::var("ZUUL_GCP_CREDENTIALS").ok());

            if let Some(raw) = raw {
                let json = resolve_credentials(&raw)?;
                let creds = google_cloud_auth::credentials::service_account::Builder::new(json)
                    .build()
                    .map_err(|e| {
                        ZuulError::Auth(format!("Failed to load service account credentials: {e}"))
                    })?;

                SecretManagerService::builder()
                    .with_credentials(creds)
                    .build()
                    .await
            } else {
                // Fall back to ADC.
                SecretManagerService::builder().build().await
            }
        }
        .map_err(|e| ZuulError::Auth(format!("Failed to authenticate with GCP: {e}.")))?;

        Ok(Self {
            client,
            project_id: project_id.to_string(),
        })
    }

    /// Returns the GCP resource path prefix for this project.
    pub fn project_path(&self) -> String {
        format!("projects/{}", self.project_id)
    }

    /// Returns the full secret resource name.
    pub fn secret_path(&self, secret_id: &str) -> String {
        format!("projects/{}/secrets/{}", self.project_id, secret_id)
    }

    /// Returns the path to the latest version of a secret.
    pub fn secret_version_path(&self, secret_id: &str) -> String {
        format!(
            "projects/{}/secrets/{}/versions/latest",
            self.project_id, secret_id
        )
    }

    /// Create a new GCP secret with the given labels and annotations.
    ///
    /// The secret is created with automatic replication. No version (value)
    /// is added — call [`add_secret_version`](Self::add_secret_version) to set the value.
    pub async fn create_secret(
        &self,
        secret_id: &str,
        labels: HashMap<String, String>,
        annotations: HashMap<String, String>,
    ) -> Result<Secret, ZuulError> {
        self.client
            .create_secret()
            .set_parent(self.project_path())
            .set_secret_id(secret_id)
            .set_secret(
                Secret::new()
                    .set_replication(Replication::new().set_automatic(Automatic::new()))
                    .set_labels(labels)
                    .set_annotations(annotations),
            )
            .send()
            .await
            .map_err(map_error)
    }

    /// Get a secret's metadata (labels, annotations, etc.).
    pub async fn get_secret(&self, secret_id: &str) -> Result<Secret, ZuulError> {
        self.client
            .get_secret()
            .set_name(self.secret_path(secret_id))
            .send()
            .await
            .map_err(map_error)
    }

    /// Delete a GCP secret and all its versions.
    pub async fn delete_secret(&self, secret_id: &str) -> Result<(), ZuulError> {
        self.client
            .delete_secret()
            .set_name(self.secret_path(secret_id))
            .send()
            .await
            .map_err(map_error)
    }

    /// List secrets matching an optional filter string.
    ///
    /// Handles pagination automatically via the SDK's built-in paginator.
    /// The filter uses GCP's filter syntax, e.g. `labels.zuul-managed=true`.
    pub async fn list_secrets(&self, filter: &str) -> Result<Vec<Secret>, ZuulError> {
        let mut items = self
            .client
            .list_secrets()
            .set_parent(self.project_path())
            .set_filter(filter)
            .by_item();

        let mut all_secrets = Vec::new();
        while let Some(result) = items.next().await {
            all_secrets.push(result.map_err(map_error)?);
        }

        Ok(all_secrets)
    }

    /// Add a new version to a secret with the given payload data.
    ///
    /// This is how secret values are set or updated — each call creates
    /// a new version, and `latest` automatically points to it.
    pub async fn add_secret_version(
        &self,
        secret_id: &str,
        payload: &[u8],
    ) -> Result<(), ZuulError> {
        self.client
            .add_secret_version()
            .set_parent(self.secret_path(secret_id))
            .set_payload(SecretPayload::new().set_data(bytes::Bytes::copy_from_slice(payload)))
            .send()
            .await
            .map_err(map_error)?;

        Ok(())
    }

    /// Access the latest version of a secret, returning the payload bytes and
    /// the full version resource name (e.g., `projects/p/secrets/s/versions/3`).
    pub async fn access_secret_version(
        &self,
        secret_id: &str,
    ) -> Result<(Vec<u8>, String), ZuulError> {
        let response = self
            .client
            .access_secret_version()
            .set_name(self.secret_version_path(secret_id))
            .send()
            .await
            .map_err(map_error)?;

        let version_name = response.name.clone();

        let payload = response
            .payload
            .ok_or_else(|| ZuulError::Backend("Secret version has no payload".to_string()))?;

        Ok((payload.data.to_vec(), version_name))
    }

    /// Update a secret's labels and/or annotations.
    ///
    /// Only the provided fields are updated; pass `None` to leave
    /// that field unchanged.
    pub async fn update_secret(
        &self,
        secret_id: &str,
        labels: Option<HashMap<String, String>>,
        annotations: Option<HashMap<String, String>>,
    ) -> Result<Secret, ZuulError> {
        let mut paths = Vec::new();
        let mut secret = Secret::new().set_name(self.secret_path(secret_id));

        if let Some(l) = labels {
            paths.push("labels".to_string());
            secret = secret.set_labels(l);
        }
        if let Some(a) = annotations {
            paths.push("annotations".to_string());
            secret = secret.set_annotations(a);
        }

        self.client
            .update_secret()
            .set_secret(secret)
            .set_update_mask(
                google_cloud_wkt::FieldMask::default().set_paths(paths.iter().map(String::as_str)),
            )
            .send()
            .await
            .map_err(map_error)
    }
}

/// Map a GCP SDK error to a `ZuulError`.
fn map_error(err: google_cloud_gax::error::Error) -> ZuulError {
    if err.is_authentication() {
        return ZuulError::Auth(
            "Authentication expired. Run `gcloud auth application-default login` to re-authenticate."
                .to_string(),
        );
    }

    if let Some(status) = err.status() {
        match status.code {
            Code::NotFound => {
                ZuulError::Backend(format!("Resource not found: {}", status.message))
            }
            Code::AlreadyExists => {
                ZuulError::Backend(format!("Resource already exists: {}", status.message))
            }
            Code::PermissionDenied => ZuulError::PermissionDenied {
                resource: status.message.clone(),
            },
            Code::Unauthenticated => ZuulError::Auth(
                "Authentication expired. Run `gcloud auth application-default login` to re-authenticate.".to_string(),
            ),
            Code::DeadlineExceeded => ZuulError::Backend(
                "Request timed out. Check your network connection and try again.".to_string(),
            ),
            Code::ResourceExhausted => ZuulError::Backend(
                "GCP rate limit exceeded. Wait a moment and try again, or check your quota at https://console.cloud.google.com/iam-admin/quotas.".to_string(),
            ),
            _ => ZuulError::Backend(format!("{:?}: {}", status.code, status.message)),
        }
    } else {
        ZuulError::Backend(format!("{err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- resolve_credentials tests ---

    #[test]
    fn resolve_credentials_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("creds.json");
        std::fs::write(
            &file_path,
            r#"{"type": "service_account", "project_id": "test"}"#,
        )
        .unwrap();

        let result = resolve_credentials(file_path.to_str().unwrap()).unwrap();
        assert_eq!(result["type"], "service_account");
        assert_eq!(result["project_id"], "test");
    }

    #[test]
    fn resolve_credentials_inline_json() {
        let json = r#"{"type": "service_account", "project_id": "test"}"#;
        let result = resolve_credentials(json).unwrap();
        assert_eq!(result["type"], "service_account");
        assert_eq!(result["project_id"], "test");
    }

    #[test]
    fn resolve_credentials_inline_json_with_whitespace() {
        let json = r#"  { "type": "service_account" }  "#;
        let result = resolve_credentials(json).unwrap();
        assert_eq!(result["type"], "service_account");
    }

    #[test]
    fn resolve_credentials_nonexistent_file_and_invalid_json() {
        let bad = "/nonexistent/path/to/creds.json";
        let result = resolve_credentials(bad);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not a valid file path"),
            "error should mention file not found, got: {err}"
        );
    }

    #[test]
    fn resolve_credentials_malformed_json() {
        let bad = "{ not valid json ";
        let result = resolve_credentials(bad);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not valid JSON"),
            "error should mention invalid JSON, got: {err}"
        );
    }

    #[test]
    fn resolve_credentials_file_with_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("bad.json");
        std::fs::write(&file_path, "not json at all").unwrap();

        let result = resolve_credentials(file_path.to_str().unwrap());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid JSON"),
            "error should mention invalid JSON in file, got: {err}"
        );
    }

    // --- map_error tests ---

    #[test]
    fn map_error_not_found() {
        let err = google_cloud_gax::error::Error::service(
            google_cloud_gax::error::rpc::Status::default()
                .set_code(Code::NotFound)
                .set_message("secret xyz"),
        );
        let zuul_err = map_error(err);
        match &zuul_err {
            ZuulError::Backend(msg) => assert!(msg.contains("not found")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn map_error_already_exists() {
        let err = google_cloud_gax::error::Error::service(
            google_cloud_gax::error::rpc::Status::default()
                .set_code(Code::AlreadyExists)
                .set_message("secret xyz"),
        );
        let zuul_err = map_error(err);
        match &zuul_err {
            ZuulError::Backend(msg) => assert!(msg.contains("already exists")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn map_error_permission_denied() {
        let err = google_cloud_gax::error::Error::service(
            google_cloud_gax::error::rpc::Status::default()
                .set_code(Code::PermissionDenied)
                .set_message("zuul__prod__DB_URL"),
        );
        let zuul_err = map_error(err);
        match &zuul_err {
            ZuulError::PermissionDenied { resource } => {
                assert!(resource.contains("zuul__prod__DB_URL"));
            }
            other => panic!("expected PermissionDenied, got {other:?}"),
        }
    }

    #[test]
    fn map_error_unauthenticated() {
        let err = google_cloud_gax::error::Error::service(
            google_cloud_gax::error::rpc::Status::default()
                .set_code(Code::Unauthenticated)
                .set_message("expired token"),
        );
        let zuul_err = map_error(err);
        match &zuul_err {
            ZuulError::Auth(msg) => {
                assert!(
                    msg.contains("gcloud auth application-default login"),
                    "should suggest re-auth command, got: {msg}"
                );
            }
            other => panic!("expected Auth, got {other:?}"),
        }
    }

    #[test]
    fn map_error_deadline_exceeded() {
        let err = google_cloud_gax::error::Error::service(
            google_cloud_gax::error::rpc::Status::default()
                .set_code(Code::DeadlineExceeded)
                .set_message("timed out"),
        );
        let zuul_err = map_error(err);
        match &zuul_err {
            ZuulError::Backend(msg) => {
                assert!(
                    msg.contains("timed out") || msg.contains("timeout"),
                    "should mention timeout, got: {msg}"
                );
            }
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn map_error_resource_exhausted() {
        let err = google_cloud_gax::error::Error::service(
            google_cloud_gax::error::rpc::Status::default()
                .set_code(Code::ResourceExhausted)
                .set_message("quota exceeded"),
        );
        let zuul_err = map_error(err);
        match &zuul_err {
            ZuulError::Backend(msg) => {
                assert!(
                    msg.contains("rate limit") || msg.contains("quota"),
                    "should mention rate limit, got: {msg}"
                );
            }
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn map_error_other_codes() {
        let err = google_cloud_gax::error::Error::service(
            google_cloud_gax::error::rpc::Status::default()
                .set_code(Code::Internal)
                .set_message("something broke"),
        );
        let zuul_err = map_error(err);
        match &zuul_err {
            ZuulError::Backend(msg) => {
                assert!(msg.contains("something broke"));
            }
            other => panic!("expected Backend, got {other:?}"),
        }
    }
}
