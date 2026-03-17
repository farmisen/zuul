use std::collections::HashMap;
use std::path::PathBuf;

use crate::backend::Backend;
use crate::error::ZuulError;
use crate::models::{Environment, SecretEntry, SecretValue};

/// File-based encrypted backend.
///
/// Stores all environments, secrets, and metadata in a single encrypted
/// JSON file using the `age` encryption format. Designed for local
/// development, small projects, and offline use.
pub struct FileBackend {
    /// Path to the encrypted store file.
    pub store_path: PathBuf,
    /// Path to an age identity file (if provided).
    pub identity: Option<PathBuf>,
}

impl FileBackend {
    /// Create a new file backend with the given store path and optional identity file.
    pub fn new(store_path: PathBuf, identity: Option<PathBuf>) -> Self {
        Self {
            store_path,
            identity,
        }
    }
}

impl Backend for FileBackend {
    // --- Environment operations ---

    async fn list_environments(&self) -> Result<Vec<Environment>, ZuulError> {
        todo!("file backend: list_environments")
    }

    async fn create_environment(
        &self,
        _name: &str,
        _description: Option<&str>,
    ) -> Result<Environment, ZuulError> {
        todo!("file backend: create_environment")
    }

    async fn get_environment(&self, _name: &str) -> Result<Environment, ZuulError> {
        todo!("file backend: get_environment")
    }

    async fn update_environment(
        &self,
        _name: &str,
        _new_name: Option<&str>,
        _new_description: Option<&str>,
    ) -> Result<Environment, ZuulError> {
        todo!("file backend: update_environment")
    }

    async fn delete_environment(&self, _name: &str) -> Result<(), ZuulError> {
        todo!("file backend: delete_environment")
    }

    // --- Secret operations ---

    async fn list_secrets(
        &self,
        _environment: Option<&str>,
    ) -> Result<Vec<SecretEntry>, ZuulError> {
        todo!("file backend: list_secrets")
    }

    async fn get_secret(&self, _name: &str, _environment: &str) -> Result<SecretValue, ZuulError> {
        todo!("file backend: get_secret")
    }

    async fn set_secret(
        &self,
        _name: &str,
        _environment: &str,
        _value: &str,
    ) -> Result<(), ZuulError> {
        todo!("file backend: set_secret")
    }

    async fn delete_secret(&self, _name: &str, _environment: &str) -> Result<(), ZuulError> {
        todo!("file backend: delete_secret")
    }

    // --- Metadata operations ---

    async fn get_metadata(
        &self,
        _name: &str,
        _environment: &str,
    ) -> Result<HashMap<String, String>, ZuulError> {
        todo!("file backend: get_metadata")
    }

    async fn set_metadata(
        &self,
        _name: &str,
        _environment: &str,
        _key: &str,
        _value: &str,
    ) -> Result<(), ZuulError> {
        todo!("file backend: set_metadata")
    }

    async fn delete_metadata(
        &self,
        _name: &str,
        _environment: &str,
        _key: &str,
    ) -> Result<(), ZuulError> {
        todo!("file backend: delete_metadata")
    }

    // --- Bulk operations ---

    async fn list_secrets_for_environment(
        &self,
        _environment: &str,
    ) -> Result<Vec<(String, SecretValue)>, ZuulError> {
        todo!("file backend: list_secrets_for_environment")
    }
}
