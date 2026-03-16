use std::collections::HashMap;
use std::env;
use std::time::Duration;

use async_trait::async_trait;
use backon::{ExponentialBuilder, Retryable};
use chrono::{Duration as ChronoDuration, Utc};
use gcloud_sdk::google::cloud::secretmanager::v1::secret_manager_service_client::SecretManagerServiceClient;
use gcloud_sdk::google::cloud::secretmanager::v1::{
    AccessSecretVersionRequest, AddSecretVersionRequest, CreateSecretRequest, DeleteSecretRequest,
    GetSecretRequest, ListSecretsRequest, Replication, Secret, UpdateSecretRequest,
};
use gcloud_sdk::prost_types::FieldMask;
use gcloud_sdk::tonic::Code;
use gcloud_sdk::{BoxSource, GoogleApi, GoogleAuthMiddleware, Source, Token, TokenSourceType};

use crate::error::ZuulError;

/// Environment variable to override the Secret Manager endpoint (for emulator use).
const EMULATOR_HOST_ENV: &str = "SECRET_MANAGER_EMULATOR_HOST";

/// Dummy token source for connecting to a Secret Manager emulator without real credentials.
struct EmulatorTokenSource;

#[async_trait]
impl Source for EmulatorTokenSource {
    async fn token(&self) -> gcloud_sdk::error::Result<Token> {
        Ok(Token::new(
            "Bearer".to_string(),
            "emulator-token".into(),
            Utc::now() + ChronoDuration::hours(1),
        ))
    }
}

/// Page size for list operations.
const LIST_PAGE_SIZE: i32 = 100;

/// Returns the retry policy used for all GCP API calls.
///
/// Retries up to 3 times with exponential backoff starting at 100ms.
fn retry_policy() -> ExponentialBuilder {
    ExponentialBuilder::default()
        .with_min_delay(Duration::from_millis(100))
        .with_max_times(3)
}

/// GCP Secret Manager client wrapper.
///
/// Handles authentication via Application Default Credentials (ADC)
/// or a service account key file. Token refresh is managed automatically
/// by the underlying `gcloud-sdk` crate.
pub struct GcpClient {
    pub(crate) client: GoogleApi<SecretManagerServiceClient<GoogleAuthMiddleware>>,
    pub(crate) project_id: String,
}

impl GcpClient {
    /// Create a new GCP Secret Manager client.
    ///
    /// Authentication is resolved in this order:
    /// 1. `credentials_path` argument (from config or CLI)
    /// 2. `ZUUL_GCP_CREDENTIALS` environment variable
    /// 3. Application Default Credentials (ADC) — e.g., from `gcloud auth application-default login`
    ///
    /// If `SECRET_MANAGER_EMULATOR_HOST` is set, connects to that endpoint
    /// with a dummy token (no real credentials needed).
    pub async fn new(project_id: &str, credentials_path: Option<&str>) -> Result<Self, ZuulError> {
        let client = if let Ok(emulator_host) = env::var(EMULATOR_HOST_ENV) {
            // Connect to emulator without real credentials.
            let dummy_source: BoxSource = Box::new(EmulatorTokenSource);
            GoogleApi::from_function_with_token_source(
                SecretManagerServiceClient::new,
                &emulator_host,
                None,
                vec!["https://www.googleapis.com/auth/cloud-platform".into()],
                TokenSourceType::ExternalSource(dummy_source),
            )
            .await
        } else {
            // Map credentials to GOOGLE_APPLICATION_CREDENTIALS so gcloud-sdk picks them up.
            // Priority: explicit path > ZUUL_GCP_CREDENTIALS env var > ADC (automatic).
            let creds = credentials_path
                .map(String::from)
                .or_else(|| env::var("ZUUL_GCP_CREDENTIALS").ok());
            if let Some(path) = creds {
                // SAFETY: called once during single-threaded client init, before tokio spawns tasks.
                unsafe {
                    env::set_var("GOOGLE_APPLICATION_CREDENTIALS", path);
                }
            }

            GoogleApi::from_function(
                SecretManagerServiceClient::new,
                "https://secretmanager.googleapis.com",
                None,
            )
            .await
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
        let request = CreateSecretRequest {
            parent: self.project_path(),
            secret_id: secret_id.to_string(),
            secret: Some(Secret {
                replication: Some(Replication {
                    replication: Some(
                        gcloud_sdk::google::cloud::secretmanager::v1::replication::Replication::Automatic(
                            gcloud_sdk::google::cloud::secretmanager::v1::replication::Automatic {
                                customer_managed_encryption: None,
                            },
                        ),
                    ),
                }),
                labels,
                annotations,
                ..Default::default()
            }),
        };

        let response =
            with_timeout(|| async { self.client.get().create_secret(request.clone()).await })
                .await?;

        Ok(response.into_inner())
    }

    /// Get a secret's metadata (labels, annotations, etc.).
    pub async fn get_secret(&self, secret_id: &str) -> Result<Secret, ZuulError> {
        let request = GetSecretRequest {
            name: self.secret_path(secret_id),
        };

        let response =
            with_timeout(|| async { self.client.get().get_secret(request.clone()).await }).await?;

        Ok(response.into_inner())
    }

    /// Delete a GCP secret and all its versions.
    pub async fn delete_secret(&self, secret_id: &str) -> Result<(), ZuulError> {
        let request = DeleteSecretRequest {
            name: self.secret_path(secret_id),
            etag: String::new(),
        };

        with_timeout(|| async { self.client.get().delete_secret(request.clone()).await }).await?;

        Ok(())
    }

    /// List secrets matching an optional filter string.
    ///
    /// Handles pagination automatically, returning all matching secrets.
    /// The filter uses GCP's filter syntax, e.g. `labels.zuul-managed=true`.
    pub async fn list_secrets(&self, filter: &str) -> Result<Vec<Secret>, ZuulError> {
        let mut all_secrets = Vec::new();
        let mut page_token = String::new();

        loop {
            let request = ListSecretsRequest {
                parent: self.project_path(),
                page_size: LIST_PAGE_SIZE,
                page_token: page_token.clone(),
                filter: filter.to_string(),
            };

            let response =
                with_timeout(|| async { self.client.get().list_secrets(request.clone()).await })
                    .await?;

            let inner = response.into_inner();
            all_secrets.extend(inner.secrets);

            if inner.next_page_token.is_empty() {
                break;
            }
            page_token = inner.next_page_token;
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
        let checksum = crc32c::crc32c(payload);
        let request = AddSecretVersionRequest {
            parent: self.secret_path(secret_id),
            payload: Some(gcloud_sdk::proto_ext::secretmanager::SecretPayload {
                data: payload.to_vec().into(),
                data_crc32c: Some(checksum as i64),
            }),
        };

        with_timeout(|| async { self.client.get().add_secret_version(request.clone()).await })
            .await?;

        Ok(())
    }

    /// Access the latest version of a secret, returning the payload bytes and
    /// the full version resource name (e.g., `projects/p/secrets/s/versions/3`).
    pub async fn access_secret_version(
        &self,
        secret_id: &str,
    ) -> Result<(Vec<u8>, String), ZuulError> {
        let request = AccessSecretVersionRequest {
            name: self.secret_version_path(secret_id),
        };

        let response = with_timeout(|| async {
            self.client
                .get()
                .access_secret_version(request.clone())
                .await
        })
        .await?;

        let inner = response.into_inner();
        let version_name = inner.name.clone();

        let payload = inner
            .payload
            .ok_or_else(|| ZuulError::Backend("Secret version has no payload".to_string()))?;

        Ok((payload.data.ref_sensitive_value().to_vec(), version_name))
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
        let mut secret = Secret {
            name: self.secret_path(secret_id),
            ..Default::default()
        };

        if let Some(l) = labels {
            paths.push("labels".to_string());
            secret.labels = l;
        }
        if let Some(a) = annotations {
            paths.push("annotations".to_string());
            secret.annotations = a;
        }

        let request = UpdateSecretRequest {
            secret: Some(secret),
            update_mask: Some(FieldMask { paths }),
        };

        let response =
            with_timeout(|| async { self.client.get().update_secret(request.clone()).await })
                .await?;

        Ok(response.into_inner())
    }
}

/// Map a gRPC `tonic::Status` to a `ZuulError`.
fn map_status(status: gcloud_sdk::tonic::Status) -> ZuulError {
    match status.code() {
        Code::NotFound => ZuulError::Backend(format!("Resource not found: {}", status.message())),
        Code::AlreadyExists => {
            ZuulError::Backend(format!("Resource already exists: {}", status.message()))
        }
        Code::PermissionDenied => ZuulError::PermissionDenied {
            resource: status.message().to_string(),
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
        _ => ZuulError::Backend(format!("{}: {}", status.code(), status.message())),
    }
}

/// Returns true if the gRPC status code is transient and worth retrying.
fn is_retryable(status: &gcloud_sdk::tonic::Status) -> bool {
    matches!(
        status.code(),
        Code::Unavailable | Code::ResourceExhausted | Code::DeadlineExceeded
    )
}

/// Timeout for individual GCP API calls (including retries).
const API_TIMEOUT: Duration = Duration::from_secs(30);

/// Run a retryable GCP operation with a timeout.
async fn with_timeout<F, Fut, T>(op: F) -> Result<T, ZuulError>
where
    F: FnMut() -> Fut + Clone,
    Fut: std::future::Future<Output = Result<T, gcloud_sdk::tonic::Status>>,
{
    tokio::time::timeout(API_TIMEOUT, op.retry(retry_policy()).when(is_retryable))
        .await
        .map_err(|_| {
            ZuulError::Backend(
                "Request timed out. Check your network connection and try again.".to_string(),
            )
        })?
        .map_err(map_status)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- map_status tests ---

    #[test]
    fn map_status_not_found() {
        let status = gcloud_sdk::tonic::Status::not_found("secret xyz");
        let err = map_status(status);
        match &err {
            ZuulError::Backend(msg) => assert!(msg.contains("not found")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn map_status_already_exists() {
        let status = gcloud_sdk::tonic::Status::already_exists("secret xyz");
        let err = map_status(status);
        match &err {
            ZuulError::Backend(msg) => assert!(msg.contains("already exists")),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn map_status_permission_denied() {
        let status = gcloud_sdk::tonic::Status::permission_denied("zuul__prod__DB_URL");
        let err = map_status(status);
        match &err {
            ZuulError::PermissionDenied { resource } => {
                assert!(resource.contains("zuul__prod__DB_URL"));
            }
            other => panic!("expected PermissionDenied, got {other:?}"),
        }
    }

    #[test]
    fn map_status_unauthenticated() {
        let status = gcloud_sdk::tonic::Status::unauthenticated("expired token");
        let err = map_status(status);
        match &err {
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
    fn map_status_deadline_exceeded() {
        let status = gcloud_sdk::tonic::Status::new(Code::DeadlineExceeded, "timed out");
        let err = map_status(status);
        match &err {
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
    fn map_status_resource_exhausted() {
        let status = gcloud_sdk::tonic::Status::resource_exhausted("quota exceeded");
        let err = map_status(status);
        match &err {
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
    fn map_status_other_codes() {
        let status = gcloud_sdk::tonic::Status::internal("something broke");
        let err = map_status(status);
        match &err {
            ZuulError::Backend(msg) => {
                assert!(msg.contains("something broke"));
            }
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    // --- is_retryable tests ---

    #[test]
    fn retryable_unavailable() {
        let status = gcloud_sdk::tonic::Status::unavailable("service down");
        assert!(is_retryable(&status));
    }

    #[test]
    fn retryable_resource_exhausted() {
        let status = gcloud_sdk::tonic::Status::resource_exhausted("rate limited");
        assert!(is_retryable(&status));
    }

    #[test]
    fn retryable_deadline_exceeded() {
        let status = gcloud_sdk::tonic::Status::new(Code::DeadlineExceeded, "timed out");
        assert!(is_retryable(&status));
    }

    #[test]
    fn not_retryable_not_found() {
        let status = gcloud_sdk::tonic::Status::not_found("gone");
        assert!(!is_retryable(&status));
    }

    #[test]
    fn not_retryable_permission_denied() {
        let status = gcloud_sdk::tonic::Status::permission_denied("nope");
        assert!(!is_retryable(&status));
    }

    #[test]
    fn not_retryable_internal() {
        let status = gcloud_sdk::tonic::Status::internal("bug");
        assert!(!is_retryable(&status));
    }
}
