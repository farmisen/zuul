use std::collections::HashMap;
use std::process::Command;

use crate::error::ZuulError;

use super::SyncTarget;

/// Valid Netlify deploy contexts.
const VALID_CONTEXTS: &[&str] = &["production", "deploy-preview", "branch-deploy", "dev"];

/// Valid Netlify scopes.
const VALID_SCOPES: &[&str] = &["builds", "functions", "runtime", "post-processing"];

/// Netlify sync target.
///
/// Shells out to the `netlify` CLI for all operations.
#[derive(Debug)]
pub struct NetlifyTarget {
    context: String,
    scopes: Vec<String>,
}

impl NetlifyTarget {
    /// Create a new Netlify sync target after validating context and scopes.
    pub fn new(context: &str, scopes: &[String]) -> Result<Self, ZuulError> {
        validate_context(context)?;
        validate_scopes(scopes)?;
        Ok(Self {
            context: context.to_string(),
            scopes: scopes.to_vec(),
        })
    }
}

impl SyncTarget for NetlifyTarget {
    fn name(&self) -> &str {
        "Netlify"
    }

    fn target_description(&self) -> String {
        format!("Netlify/{}", self.context)
    }

    fn list_vars(&self) -> Result<HashMap<String, String>, ZuulError> {
        let output = Command::new("netlify")
            .args(["env:list", "--json", "--context", &self.context])
            .output()
            .map_err(|e| {
                ZuulError::Backend(format!(
                    "Failed to run 'netlify env:list': {e}. Is the Netlify CLI installed?"
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZuulError::Backend(format!(
                "netlify env:list failed: {stderr}"
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_env_list(&stdout, &self.context)
    }

    fn set_var(&self, name: &str, value: &str) -> Result<(), ZuulError> {
        let mut args = vec!["env:set", name, value, "--context", &self.context];

        // Add scopes — netlify CLI accepts --scope for each scope
        let scope_refs: Vec<&str> = self.scopes.iter().map(String::as_str).collect();
        for scope in &scope_refs {
            args.push("--scope");
            args.push(scope);
        }

        let output = Command::new("netlify").args(&args).output().map_err(|e| {
            ZuulError::Backend(format!(
                "Failed to run 'netlify env:set': {e}. Is the Netlify CLI installed?"
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZuulError::Backend(format!(
                "netlify env:set {name} failed: {stderr}"
            )));
        }

        Ok(())
    }

    fn unset_var(&self, name: &str) -> Result<(), ZuulError> {
        let output = Command::new("netlify")
            .args(["env:unset", name, "--context", &self.context])
            .output()
            .map_err(|e| {
                ZuulError::Backend(format!(
                    "Failed to run 'netlify env:unset': {e}. Is the Netlify CLI installed?"
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZuulError::Backend(format!(
                "netlify env:unset {name} failed: {stderr}"
            )));
        }

        Ok(())
    }
}

/// Validate that the context is a valid Netlify deploy context.
///
/// Accepts the standard contexts (`production`, `deploy-preview`, `branch-deploy`, `dev`)
/// plus branch-specific contexts in the form `branch:<name>`.
fn validate_context(context: &str) -> Result<(), ZuulError> {
    if VALID_CONTEXTS.contains(&context) || context.starts_with("branch:") {
        return Ok(());
    }
    Err(ZuulError::Validation(format!(
        "Invalid Netlify context '{context}'. Valid contexts: {}, branch:<name>",
        VALID_CONTEXTS.join(", ")
    )))
}

/// Validate that all scopes are valid Netlify scopes.
fn validate_scopes(scopes: &[String]) -> Result<(), ZuulError> {
    for scope in scopes {
        if !VALID_SCOPES.contains(&scope.as_str()) {
            return Err(ZuulError::Validation(format!(
                "Invalid Netlify scope '{scope}'. Valid scopes: {}",
                VALID_SCOPES.join(", ")
            )));
        }
    }
    Ok(())
}

/// Parse the JSON output from `netlify env:list --json`.
///
/// The output is an array of env var objects with nested context-specific values:
/// ```json
/// [{"key": "VAR", "values": [{"value": "val", "context": "production"}, ...]}]
/// ```
///
/// The `context` parameter filters which value entry to use. If no value matches
/// the exact context, the variable is skipped (it doesn't exist in that context).
fn parse_env_list(json_output: &str, context: &str) -> Result<HashMap<String, String>, ZuulError> {
    let parsed: serde_json::Value = serde_json::from_str(json_output)
        .map_err(|e| ZuulError::Backend(format!("Failed to parse netlify env:list output: {e}")))?;

    let arr = parsed.as_array().ok_or_else(|| {
        ZuulError::Backend("netlify env:list output is not a JSON array".to_string())
    })?;

    let mut vars = HashMap::new();
    for item in arr {
        let key = match item.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => continue,
        };

        // Find the value entry matching the requested context.
        // Fall back to "all" context if no exact match (Netlify uses "all" for vars
        // that apply to every context).
        let values = match item.get("values").and_then(|v| v.as_array()) {
            Some(v) => v,
            None => continue,
        };

        let value = values
            .iter()
            .find(|v| v.get("context").and_then(|c| c.as_str()) == Some(context))
            .or_else(|| {
                values
                    .iter()
                    .find(|v| v.get("context").and_then(|c| c.as_str()) == Some("all"))
            })
            .and_then(|v| v.get("value").and_then(|val| val.as_str()));

        if let Some(val) = value {
            vars.insert(key.to_string(), val.to_string());
        }
    }

    Ok(vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_valid() {
        let target = NetlifyTarget::new("production", &["builds".into(), "functions".into()]);
        assert!(target.is_ok());
    }

    #[test]
    fn new_invalid_context() {
        let err = NetlifyTarget::new("staging", &["builds".into()]).unwrap_err();
        assert!(err.to_string().contains("Invalid Netlify context"));
    }

    #[test]
    fn new_invalid_scope() {
        let err = NetlifyTarget::new("production", &["invalid".into()]).unwrap_err();
        assert!(err.to_string().contains("Invalid Netlify scope"));
    }

    #[test]
    fn target_description_format() {
        let target = NetlifyTarget::new("production", &["builds".into()]).unwrap();
        assert_eq!(target.target_description(), "Netlify/production");
    }

    #[test]
    fn new_branch_context_valid() {
        let target = NetlifyTarget::new("branch:staging", &["builds".into()]);
        assert!(target.is_ok());
        assert_eq!(
            target.unwrap().target_description(),
            "Netlify/branch:staging"
        );
    }

    #[test]
    fn parse_env_list_basic() {
        let json = r#"[
            {"key": "DATABASE_URL", "values": [
                {"value": "postgres://prod", "context": "production"},
                {"value": "postgres://dev", "context": "dev"}
            ]},
            {"key": "API_KEY", "values": [
                {"value": "sk_live_123", "context": "production"}
            ]}
        ]"#;
        let vars = parse_env_list(json, "production").unwrap();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars["DATABASE_URL"], "postgres://prod");
        assert_eq!(vars["API_KEY"], "sk_live_123");
    }

    #[test]
    fn parse_env_list_filters_by_context() {
        let json = r#"[
            {"key": "DB", "values": [
                {"value": "prod-db", "context": "production"},
                {"value": "dev-db", "context": "dev"}
            ]}
        ]"#;
        let prod = parse_env_list(json, "production").unwrap();
        assert_eq!(prod["DB"], "prod-db");

        let dev = parse_env_list(json, "dev").unwrap();
        assert_eq!(dev["DB"], "dev-db");
    }

    #[test]
    fn parse_env_list_falls_back_to_all_context() {
        let json = r#"[
            {"key": "SHARED", "values": [
                {"value": "shared-val", "context": "all"}
            ]}
        ]"#;
        let vars = parse_env_list(json, "production").unwrap();
        assert_eq!(vars["SHARED"], "shared-val");
    }

    #[test]
    fn parse_env_list_skips_missing_context() {
        let json = r#"[
            {"key": "PROD_ONLY", "values": [
                {"value": "val", "context": "production"}
            ]}
        ]"#;
        let vars = parse_env_list(json, "dev").unwrap();
        assert!(vars.is_empty());
    }

    #[test]
    fn parse_env_list_empty() {
        let vars = parse_env_list("[]", "production").unwrap();
        assert!(vars.is_empty());
    }

    #[test]
    fn parse_env_list_invalid_json() {
        let err = parse_env_list("not json", "production").unwrap_err();
        assert!(err.to_string().contains("Failed to parse"));
    }
}
