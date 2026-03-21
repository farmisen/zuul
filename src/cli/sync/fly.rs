use std::collections::HashMap;
use std::process::Command;

use crate::error::ZuulError;

use super::SyncTarget;

/// Fly.io sync target.
///
/// Shells out to the `fly` CLI for all operations. Fly has a flat secret model —
/// no contexts or scopes. Secret values cannot be read back via the CLI, so sync
/// always pushes all secrets (cannot diff values).
#[derive(Debug)]
pub struct FlyTarget {
    app: Option<String>,
    stage: bool,
}

impl FlyTarget {
    /// Create a new Fly.io sync target.
    ///
    /// If `app` is `None`, the `fly` CLI auto-detects from `fly.toml` in the
    /// current directory.
    pub fn new(app: Option<&str>, stage: bool) -> Self {
        Self {
            app: app.map(String::from),
            stage,
        }
    }

    /// Build the base args for a fly command, including `--app` if set.
    fn base_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(app) = &self.app {
            args.push("--app".to_string());
            args.push(app.clone());
        }
        args
    }

    /// Resolve the app name for display. If not explicitly set, reads from fly CLI.
    fn resolve_app_name(&self) -> String {
        if let Some(app) = &self.app {
            return app.clone();
        }
        // Try to detect from `fly status --json`
        let mut args = vec!["status".to_string(), "--json".to_string()];
        args.extend(self.base_args());
        if let Some(output) = Command::new("fly").args(&args).output().ok()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&stdout)
                && let Some(name) = parsed.get("Name").and_then(|v| v.as_str())
            {
                return name.to_string();
            }
        }
        "(auto-detected)".to_string()
    }
}

impl SyncTarget for FlyTarget {
    fn name(&self) -> &str {
        "Fly"
    }

    fn target_description(&self) -> String {
        format!("Fly/{}", self.resolve_app_name())
    }

    /// List secret names from Fly.
    ///
    /// Fly does not return secret values, only names and digests.
    /// Values are returned as empty strings — the sync engine will treat
    /// all secrets as needing to be set.
    fn list_vars(&self) -> Result<HashMap<String, String>, ZuulError> {
        let mut args = vec![
            "secrets".to_string(),
            "list".to_string(),
            "--json".to_string(),
        ];
        args.extend(self.base_args());

        let output = Command::new("fly").args(&args).output().map_err(|e| {
            ZuulError::Backend(format!(
                "Failed to run 'fly secrets list': {e}. Is the Fly CLI installed?"
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZuulError::Backend(format!(
                "fly secrets list failed: {stderr}"
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_secrets_list(&stdout)
    }

    /// Set a secret on Fly.
    ///
    /// Uses `fly secrets set KEY=VALUE [--stage]`.
    fn set_var(&self, name: &str, value: &str) -> Result<(), ZuulError> {
        let mut args = vec!["secrets".to_string(), "set".to_string()];
        args.push(format!("{name}={value}"));
        if self.stage {
            args.push("--stage".to_string());
        }
        args.extend(self.base_args());

        let output = Command::new("fly").args(&args).output().map_err(|e| {
            ZuulError::Backend(format!(
                "Failed to run 'fly secrets set': {e}. Is the Fly CLI installed?"
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZuulError::Backend(format!(
                "fly secrets set {name} failed: {stderr}"
            )));
        }

        Ok(())
    }

    /// Unset a secret on Fly.
    ///
    /// Uses `fly secrets unset KEY [--stage]`.
    fn unset_var(&self, name: &str) -> Result<(), ZuulError> {
        let mut args = vec!["secrets".to_string(), "unset".to_string(), name.to_string()];
        if self.stage {
            args.push("--stage".to_string());
        }
        args.extend(self.base_args());

        let output = Command::new("fly").args(&args).output().map_err(|e| {
            ZuulError::Backend(format!(
                "Failed to run 'fly secrets unset': {e}. Is the Fly CLI installed?"
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZuulError::Backend(format!(
                "fly secrets unset {name} failed: {stderr}"
            )));
        }

        Ok(())
    }
}

/// Parse the JSON output from `fly secrets list --json`.
///
/// The output is an array of objects with `Name` and `Digest` fields (PascalCase).
/// Since Fly never returns actual values, all values are set to a sentinel
/// that will never match a real zuul secret value, ensuring every secret
/// is treated as needing to be set.
fn parse_secrets_list(json_output: &str) -> Result<HashMap<String, String>, ZuulError> {
    let parsed: serde_json::Value = serde_json::from_str(json_output)
        .map_err(|e| ZuulError::Backend(format!("Failed to parse fly secrets list output: {e}")))?;

    let arr = parsed.as_array().ok_or_else(|| {
        ZuulError::Backend("fly secrets list output is not a JSON array".to_string())
    })?;

    let mut vars = HashMap::new();
    for item in arr {
        if let Some(name) = item.get("Name").and_then(|v| v.as_str()) {
            // Use a sentinel value that no real secret would match.
            // This ensures compute_diff always generates Update actions
            // for existing secrets (since we can't read the actual value).
            vars.insert(name.to_string(), "\x00__fly_unknown__".to_string());
        }
    }

    Ok(vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_without_app() {
        let target = FlyTarget::new(None, false);
        assert!(target.app.is_none());
        assert!(!target.stage);
    }

    #[test]
    fn new_with_app_and_stage() {
        let target = FlyTarget::new(Some("my-app"), true);
        assert_eq!(target.app.as_deref(), Some("my-app"));
        assert!(target.stage);
        assert_eq!(target.target_description(), "Fly/my-app");
    }

    #[test]
    fn parse_secrets_list_basic() {
        let json = r#"[
            {"Name": "DATABASE_URL", "Digest": "f9cf94aa488eef36", "CreatedAt": "2026-01-01T00:00:00Z"},
            {"Name": "API_KEY", "Digest": "e1ee4f384787527f", "CreatedAt": "2026-01-02T00:00:00Z"}
        ]"#;
        let vars = parse_secrets_list(json).unwrap();
        assert_eq!(vars.len(), 2);
        assert!(vars.contains_key("DATABASE_URL"));
        assert!(vars.contains_key("API_KEY"));
        // Values are sentinels, not real values
        assert!(vars["DATABASE_URL"].contains("fly_unknown"));
    }

    #[test]
    fn parse_secrets_list_empty() {
        let vars = parse_secrets_list("[]").unwrap();
        assert!(vars.is_empty());
    }

    #[test]
    fn parse_secrets_list_invalid_json() {
        let err = parse_secrets_list("not json").unwrap_err();
        assert!(err.to_string().contains("Failed to parse"));
    }

    #[test]
    fn sentinel_never_matches_real_value() {
        let json = r#"[{"Name": "KEY", "Digest": "abc123"}]"#;
        let fly_vars = parse_secrets_list(json).unwrap();
        // Any real zuul value will differ from the sentinel
        assert_ne!(fly_vars["KEY"], "my-real-secret");
        assert_ne!(fly_vars["KEY"], "");
        assert_ne!(fly_vars["KEY"], "null");
    }
}
