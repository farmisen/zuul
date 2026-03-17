pub mod file_backend;
pub mod gcp;
pub mod gcp_backend;

use std::collections::HashMap;

use crate::error::ZuulError;
use crate::models::{Environment, SecretEntry, SecretValue};

/// Trait that all secret storage backends must implement.
///
/// Each method returns `Result<T, ZuulError>` so that CLI logic
/// can handle errors uniformly regardless of the underlying backend.
pub trait Backend: Send + Sync {
    // --- Environment operations ---

    /// List all known environments.
    fn list_environments(&self)
    -> impl Future<Output = Result<Vec<Environment>, ZuulError>> + Send;

    /// Create a new environment with an optional description.
    ///
    /// Backends that delegate environment management to external tools (e.g.,
    /// GCP + Terraform) should return an actionable error.
    fn create_environment(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> impl Future<Output = Result<Environment, ZuulError>> + Send;

    /// Get a single environment by name.
    fn get_environment(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Environment, ZuulError>> + Send;

    /// Update an environment's name and/or description.
    ///
    /// Backends that delegate environment management to external tools (e.g.,
    /// GCP + Terraform) should return an actionable error.
    fn update_environment(
        &self,
        name: &str,
        new_name: Option<&str>,
        new_description: Option<&str>,
    ) -> impl Future<Output = Result<Environment, ZuulError>> + Send;

    /// Delete an environment.
    ///
    /// Backends that delegate environment management to external tools (e.g.,
    /// GCP + Terraform) should return an actionable error.
    fn delete_environment(&self, name: &str) -> impl Future<Output = Result<(), ZuulError>> + Send;

    // --- Secret operations ---

    /// List secrets, optionally filtered by environment.
    fn list_secrets(
        &self,
        environment: Option<&str>,
    ) -> impl Future<Output = Result<Vec<SecretEntry>, ZuulError>> + Send;

    /// Get a secret's value in a specific environment.
    fn get_secret(
        &self,
        name: &str,
        environment: &str,
    ) -> impl Future<Output = Result<SecretValue, ZuulError>> + Send;

    /// Set (create or update) a secret's value in a specific environment.
    fn set_secret(
        &self,
        name: &str,
        environment: &str,
        value: &str,
    ) -> impl Future<Output = Result<(), ZuulError>> + Send;

    /// Delete a secret from a specific environment.
    fn delete_secret(
        &self,
        name: &str,
        environment: &str,
    ) -> impl Future<Output = Result<(), ZuulError>> + Send;

    // --- Metadata operations ---

    /// Get all metadata for a secret in a specific environment.
    fn get_metadata(
        &self,
        name: &str,
        environment: &str,
    ) -> impl Future<Output = Result<HashMap<String, String>, ZuulError>> + Send;

    /// Set a single metadata key-value pair on a secret.
    fn set_metadata(
        &self,
        name: &str,
        environment: &str,
        key: &str,
        value: &str,
    ) -> impl Future<Output = Result<(), ZuulError>> + Send;

    /// Delete a single metadata key from a secret.
    fn delete_metadata(
        &self,
        name: &str,
        environment: &str,
        key: &str,
    ) -> impl Future<Output = Result<(), ZuulError>> + Send;

    // --- Bulk operations ---

    /// List all secrets with their values for a given environment.
    fn list_secrets_for_environment(
        &self,
        environment: &str,
    ) -> impl Future<Output = Result<Vec<(String, SecretValue)>, ZuulError>> + Send;
}

/// Concrete backend dispatcher.
///
/// Wraps the available backend implementations so that `main.rs` can work
/// with a single type regardless of which backend is configured.
pub enum BackendKind {
    Gcp(gcp_backend::GcpBackend),
    File(file_backend::FileBackend),
}

/// Delegate every `Backend` method to the inner implementation.
macro_rules! delegate {
    ($self:ident, $method:ident $(, $arg:expr)*) => {
        match $self {
            BackendKind::Gcp(b) => b.$method($($arg),*).await,
            BackendKind::File(b) => b.$method($($arg),*).await,
        }
    };
}

impl Backend for BackendKind {
    async fn list_environments(&self) -> Result<Vec<Environment>, ZuulError> {
        delegate!(self, list_environments)
    }

    async fn create_environment(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<Environment, ZuulError> {
        delegate!(self, create_environment, name, description)
    }

    async fn get_environment(&self, name: &str) -> Result<Environment, ZuulError> {
        delegate!(self, get_environment, name)
    }

    async fn update_environment(
        &self,
        name: &str,
        new_name: Option<&str>,
        new_description: Option<&str>,
    ) -> Result<Environment, ZuulError> {
        delegate!(self, update_environment, name, new_name, new_description)
    }

    async fn delete_environment(&self, name: &str) -> Result<(), ZuulError> {
        delegate!(self, delete_environment, name)
    }

    async fn list_secrets(&self, environment: Option<&str>) -> Result<Vec<SecretEntry>, ZuulError> {
        delegate!(self, list_secrets, environment)
    }

    async fn get_secret(&self, name: &str, environment: &str) -> Result<SecretValue, ZuulError> {
        delegate!(self, get_secret, name, environment)
    }

    async fn set_secret(
        &self,
        name: &str,
        environment: &str,
        value: &str,
    ) -> Result<(), ZuulError> {
        delegate!(self, set_secret, name, environment, value)
    }

    async fn delete_secret(&self, name: &str, environment: &str) -> Result<(), ZuulError> {
        delegate!(self, delete_secret, name, environment)
    }

    async fn get_metadata(
        &self,
        name: &str,
        environment: &str,
    ) -> Result<HashMap<String, String>, ZuulError> {
        delegate!(self, get_metadata, name, environment)
    }

    async fn set_metadata(
        &self,
        name: &str,
        environment: &str,
        key: &str,
        value: &str,
    ) -> Result<(), ZuulError> {
        delegate!(self, set_metadata, name, environment, key, value)
    }

    async fn delete_metadata(
        &self,
        name: &str,
        environment: &str,
        key: &str,
    ) -> Result<(), ZuulError> {
        delegate!(self, delete_metadata, name, environment, key)
    }

    async fn list_secrets_for_environment(
        &self,
        environment: &str,
    ) -> Result<Vec<(String, SecretValue)>, ZuulError> {
        delegate!(self, list_secrets_for_environment, environment)
    }
}
