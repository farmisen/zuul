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
    fn update_environment(
        &self,
        name: &str,
        new_name: Option<&str>,
        new_description: Option<&str>,
    ) -> impl Future<Output = Result<Environment, ZuulError>> + Send;

    /// Delete an environment and all its bound secrets.
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
