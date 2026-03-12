use std::env;

use gcloud_sdk::google::cloud::secretmanager::v1::secret_manager_service_client::SecretManagerServiceClient;
use gcloud_sdk::{GoogleApi, GoogleAuthMiddleware};

use crate::error::ZuulError;

/// GCP Secret Manager client wrapper.
///
/// Handles authentication via Application Default Credentials (ADC)
/// or a service account key file. Token refresh is managed automatically
/// by the underlying `gcloud-sdk` crate.
#[allow(dead_code)] // Fields used in 1.2 (GCP client wrapper)
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
    pub async fn new(project_id: &str, credentials_path: Option<&str>) -> Result<Self, ZuulError> {
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

        let client = GoogleApi::from_function(
            SecretManagerServiceClient::new,
            "https://secretmanager.googleapis.com",
            None,
        )
        .await
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
}
