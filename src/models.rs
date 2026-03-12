use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A named deployment context (e.g., `production`, `staging`, `dev`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    /// Environment name. Skipped in serialization (used as map key in Registry).
    #[serde(skip)]
    pub name: String,
    /// Optional human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Summary of a secret across environments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEntry {
    pub name: String,
    pub environments: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// A secret's value in a specific environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretValue {
    pub name: String,
    pub environment: String,
    pub value: String,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Project-level registry of environments, stored as a backend secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub environments: HashMap<String, Environment>,
}

/// Maximum length for environment names.
const MAX_ENV_NAME_LEN: usize = 50;
/// Maximum length for secret names.
const MAX_SECRET_NAME_LEN: usize = 200;
/// Reserved environment names.
const RESERVED_ENV_NAMES: &[&str] = &["registry", "config"];

/// Validate an environment name against the spec constraints.
///
/// Rules:
/// - Must match `[a-z0-9][a-z0-9-]*`
/// - Must not contain `__`
/// - Max length: 50 characters
/// - Names `registry` and `config` are reserved
pub fn validate_environment_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Environment name cannot be empty".to_string());
    }
    if name.len() > MAX_ENV_NAME_LEN {
        return Err(format!(
            "Environment name '{name}' exceeds maximum length of {MAX_ENV_NAME_LEN} characters"
        ));
    }
    if name.contains("__") {
        return Err(format!(
            "Environment name '{name}' is invalid: names cannot contain '__' (reserved as delimiter)"
        ));
    }
    if RESERVED_ENV_NAMES.contains(&name) {
        return Err(format!("Environment name '{name}' is reserved"));
    }

    let mut chars = name.chars();
    let first = chars.next().unwrap(); // safe: checked non-empty above
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(format!(
            "Environment name '{name}' must start with a lowercase letter or digit"
        ));
    }
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
            return Err(format!(
                "Environment name '{name}' contains invalid character '{c}': \
                 only lowercase letters, digits, and hyphens are allowed"
            ));
        }
    }

    Ok(())
}

/// Validate a secret name against the spec constraints.
///
/// Rules:
/// - Must match `[A-Za-z_][A-Za-z0-9_-]*`
/// - Must not contain `__`
/// - Max length: 200 characters
pub fn validate_secret_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Secret name cannot be empty".to_string());
    }
    if name.len() > MAX_SECRET_NAME_LEN {
        return Err(format!(
            "Secret name '{name}' exceeds maximum length of {MAX_SECRET_NAME_LEN} characters"
        ));
    }
    if name.contains("__") {
        return Err(format!(
            "Secret name '{name}' is invalid: names cannot contain '__' (reserved as delimiter)"
        ));
    }

    let mut chars = name.chars();
    let first = chars.next().unwrap(); // safe: checked non-empty above
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(format!(
            "Secret name '{name}' must start with a letter or underscore"
        ));
    }
    for c in chars {
        if !c.is_ascii_alphanumeric() && c != '_' && c != '-' {
            return Err(format!(
                "Secret name '{name}' contains invalid character '{c}': \
                 only letters, digits, underscores, and hyphens are allowed"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Environment name validation ---

    #[test]
    fn valid_environment_names() {
        let valid = &[
            "dev",
            "staging",
            "production",
            "us-east-1",
            "a",
            "1",
            "1a",
            "a1",
            "dev-2",
        ];
        for name in valid {
            assert!(
                validate_environment_name(name).is_ok(),
                "expected '{name}' to be valid"
            );
        }
    }

    #[test]
    fn env_name_empty() {
        assert!(validate_environment_name("").is_err());
    }

    #[test]
    fn env_name_too_long() {
        let long = "a".repeat(51);
        assert!(validate_environment_name(&long).is_err());

        let at_limit = "a".repeat(50);
        assert!(validate_environment_name(&at_limit).is_ok());
    }

    #[test]
    fn env_name_double_underscore() {
        assert!(validate_environment_name("my__env").is_err());
    }

    #[test]
    fn env_name_reserved() {
        assert!(validate_environment_name("registry").is_err());
        assert!(validate_environment_name("config").is_err());
    }

    #[test]
    fn env_name_leading_hyphen() {
        assert!(validate_environment_name("-dev").is_err());
    }

    #[test]
    fn env_name_uppercase() {
        assert!(validate_environment_name("Dev").is_err());
        assert!(validate_environment_name("DEV").is_err());
    }

    #[test]
    fn env_name_special_chars() {
        assert!(validate_environment_name("dev.1").is_err());
        assert!(validate_environment_name("dev_1").is_err());
        assert!(validate_environment_name("dev 1").is_err());
    }

    // --- Secret name validation ---

    #[test]
    fn valid_secret_names() {
        let valid = &[
            "DATABASE_URL",
            "STRIPE_KEY",
            "a",
            "A",
            "_PRIVATE",
            "my-secret",
            "My_Secret-1",
            "_",
        ];
        for name in valid {
            assert!(
                validate_secret_name(name).is_ok(),
                "expected '{name}' to be valid"
            );
        }
    }

    #[test]
    fn secret_name_empty() {
        assert!(validate_secret_name("").is_err());
    }

    #[test]
    fn secret_name_too_long() {
        let long = "A".repeat(201);
        assert!(validate_secret_name(&long).is_err());

        let at_limit = "A".repeat(200);
        assert!(validate_secret_name(&at_limit).is_ok());
    }

    #[test]
    fn secret_name_double_underscore() {
        assert!(validate_secret_name("MY__SECRET").is_err());
    }

    #[test]
    fn secret_name_starts_with_digit() {
        assert!(validate_secret_name("1SECRET").is_err());
    }

    #[test]
    fn secret_name_starts_with_hyphen() {
        assert!(validate_secret_name("-SECRET").is_err());
    }

    #[test]
    fn secret_name_special_chars() {
        assert!(validate_secret_name("MY.SECRET").is_err());
        assert!(validate_secret_name("MY SECRET").is_err());
        assert!(validate_secret_name("MY@SECRET").is_err());
    }
}
