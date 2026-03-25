use std::fmt;

use thiserror::Error;

/// Application-level error type with actionable user-facing messages.
#[derive(Debug, Error)]
pub enum ZuulError {
    /// A requested resource (secret or environment) was not found.
    NotFound {
        resource_type: ResourceType,
        name: String,
        environment: Option<String>,
    },

    /// A resource already exists when trying to create it.
    AlreadyExists {
        resource_type: ResourceType,
        name: String,
        environment: Option<String>,
    },

    /// The caller lacks permission for the requested operation.
    PermissionDenied { resource: String },

    /// Input validation failed.
    Validation(String),

    /// An error from the backend (GCP, etc.).
    Backend(String),

    /// Configuration error (missing file, bad values, etc.).
    Config(String),

    /// Authentication error (missing or expired credentials).
    Auth(String),

    /// The operation is not supported by the current backend.
    Unsupported(String),
}

/// The type of resource referenced in an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Secret,
    Environment,
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceType::Secret => write!(f, "Secret"),
            ResourceType::Environment => write!(f, "Environment"),
        }
    }
}

impl fmt::Display for ZuulError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZuulError::NotFound {
                resource_type,
                name,
                environment,
            } => match (resource_type, environment) {
                (ResourceType::Secret, Some(env)) => write!(
                    f,
                    "Secret '{name}' not found in environment '{env}'. \
                     Run 'zuul secret list --env {env}' to see available secrets."
                ),
                (ResourceType::Environment, _) => write!(
                    f,
                    "Environment '{name}' does not exist. \
                     Run 'zuul env list' to see available environments."
                ),
                (ResourceType::Secret, None) => write!(
                    f,
                    "Secret '{name}' not found. \
                     Run 'zuul secret list' to see available secrets."
                ),
            },

            ZuulError::AlreadyExists {
                resource_type,
                name,
                environment,
            } => match (resource_type, environment) {
                (ResourceType::Secret, Some(env)) => {
                    write!(f, "Secret '{name}' already exists in environment '{env}'.")
                }
                (ResourceType::Environment, _) => {
                    write!(f, "Environment '{name}' already exists.")
                }
                (ResourceType::Secret, None) => {
                    write!(f, "Secret '{name}' already exists.")
                }
            },

            ZuulError::PermissionDenied { resource } => write!(
                f,
                "Permission denied for '{resource}'. \
                 Ensure you have the required permissions for the configured backend."
            ),

            ZuulError::Validation(msg) => write!(f, "{msg}"),

            ZuulError::Backend(msg) => write!(f, "Backend error: {msg}"),

            ZuulError::Config(msg) => write!(f, "{msg}"),

            ZuulError::Auth(msg) => write!(f, "{msg} Run 'zuul auth' to set up authentication."),

            ZuulError::Unsupported(msg) => write!(f, "{msg}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_secret_with_env() {
        let err = ZuulError::NotFound {
            resource_type: ResourceType::Secret,
            name: "DB_URL".to_string(),
            environment: Some("production".to_string()),
        };
        let msg = err.to_string();
        assert!(msg.contains("Secret 'DB_URL' not found in environment 'production'"));
        assert!(msg.contains("zuul secret list --env production"));
    }

    #[test]
    fn not_found_secret_without_env() {
        let err = ZuulError::NotFound {
            resource_type: ResourceType::Secret,
            name: "DB_URL".to_string(),
            environment: None,
        };
        let msg = err.to_string();
        assert!(msg.contains("Secret 'DB_URL' not found"));
        assert!(msg.contains("zuul secret list"));
    }

    #[test]
    fn not_found_environment() {
        let err = ZuulError::NotFound {
            resource_type: ResourceType::Environment,
            name: "qa".to_string(),
            environment: None,
        };
        let msg = err.to_string();
        assert!(msg.contains("Environment 'qa' does not exist"));
        assert!(msg.contains("zuul env list"));
    }

    #[test]
    fn already_exists_secret() {
        let err = ZuulError::AlreadyExists {
            resource_type: ResourceType::Secret,
            name: "API_KEY".to_string(),
            environment: Some("dev".to_string()),
        };
        let msg = err.to_string();
        assert!(msg.contains("Secret 'API_KEY' already exists in environment 'dev'"));
    }

    #[test]
    fn already_exists_environment() {
        let err = ZuulError::AlreadyExists {
            resource_type: ResourceType::Environment,
            name: "staging".to_string(),
            environment: None,
        };
        let msg = err.to_string();
        assert!(msg.contains("Environment 'staging' already exists"));
    }

    #[test]
    fn permission_denied() {
        let err = ZuulError::PermissionDenied {
            resource: "zuul__production__DB_URL".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Permission denied"));
        assert!(msg.contains("zuul__production__DB_URL"));
        assert!(msg.contains("required permissions"));
    }

    #[test]
    fn validation_error() {
        let err = ZuulError::Validation(
            "Secret name 'my__secret' is invalid: names cannot contain '__' (reserved as delimiter).".to_string(),
        );
        let msg = err.to_string();
        assert!(msg.contains("my__secret"));
        assert!(msg.contains("__"));
    }

    #[test]
    fn backend_error() {
        let err = ZuulError::Backend("connection timed out".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Backend error: connection timed out"));
    }

    #[test]
    fn config_error() {
        let err =
            ZuulError::Config("No .zuul.toml found. Run 'zuul init' to create one.".to_string());
        let msg = err.to_string();
        assert!(msg.contains("No .zuul.toml found"));
        assert!(msg.contains("zuul init"));
    }

    #[test]
    fn auth_error() {
        let err = ZuulError::Auth("No valid credentials found.".to_string());
        let msg = err.to_string();
        assert!(msg.contains("No valid credentials found"));
        assert!(msg.contains("zuul auth"));
    }

    #[test]
    fn unsupported_error() {
        let err =
            ZuulError::Unsupported("audit is not available for the file backend.".to_string());
        let msg = err.to_string();
        assert!(msg.contains("not available"));
    }
}
