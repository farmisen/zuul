use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::backend::Backend;
use crate::backend::gcp_backend::GcpBackend;
use crate::cli::ImportFormat;
use crate::error::ZuulError;

/// Run `zuul import`.
///
/// Parses secrets from a file, checks for existing secrets, and creates/updates
/// them in the target environment. Supports `--dry-run` and `--overwrite`.
pub async fn run(
    backend: &GcpBackend,
    env: &str,
    file: &Path,
    format: Option<&ImportFormat>,
    overwrite: bool,
    dry_run: bool,
) -> Result<(), ZuulError> {
    // Resolve format: explicit flag or auto-detect from extension
    let resolved_format = match format {
        Some(f) => f.clone(),
        None => detect_format(file)?,
    };

    // Read and parse the file
    let content = fs::read_to_string(file)
        .map_err(|e| ZuulError::Config(format!("Failed to read '{}': {e}", file.display())))?;

    let secrets = parse(&resolved_format, &content)?;

    if secrets.is_empty() {
        println!("No secrets found in '{}'.", file.display());
        return Ok(());
    }

    // Fetch existing secrets for this environment to detect collisions
    let existing: HashMap<String, ()> = backend
        .list_secrets(Some(env))
        .await?
        .into_iter()
        .map(|entry| (entry.name, ()))
        .collect();

    let mut created = 0u32;
    let mut overwritten = 0u32;
    let mut skipped = 0u32;

    for (name, value) in &secrets {
        let exists = existing.contains_key(name);

        if exists && !overwrite {
            skipped += 1;
            eprintln!(
                "Warning: secret '{name}' already exists, skipping (use --overwrite to replace)"
            );
            continue;
        }

        if dry_run {
            if exists {
                println!("  overwrite: {name}");
                overwritten += 1;
            } else {
                println!("  create: {name}");
                created += 1;
            }
        } else {
            backend.set_secret(name, env, value).await?;
            if exists {
                overwritten += 1;
            } else {
                created += 1;
            }
        }
    }

    // Summary
    let total = created + overwritten;
    let action = if dry_run { "Would import" } else { "Imported" };
    println!(
        "{action} {total} secrets ({skipped} skipped, {overwritten} overwritten) into environment '{env}'."
    );

    Ok(())
}

/// Auto-detect import format from file extension.
fn detect_format(path: &Path) -> Result<ImportFormat, ZuulError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "json" => Ok(ImportFormat::Json),
        "yaml" | "yml" => Ok(ImportFormat::Yaml),
        _ => Ok(ImportFormat::Dotenv),
    }
}

/// Parse file content into key-value pairs based on format.
fn parse(format: &ImportFormat, content: &str) -> Result<Vec<(String, String)>, ZuulError> {
    match format {
        ImportFormat::Dotenv => parse_dotenv(content),
        ImportFormat::Json => parse_json(content),
        ImportFormat::Yaml => parse_yaml(content),
    }
}

/// Parse dotenv format.
///
/// Handles `KEY=VALUE`, quoted values (`"..."`, `'...'`), comments (`#`),
/// blank lines, optional `export` prefix, and multiline quoted values.
fn parse_dotenv(content: &str) -> Result<Vec<(String, String)>, ZuulError> {
    let mut secrets = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Skip blank lines and comments
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        // Strip optional `export ` prefix
        let line = line.strip_prefix("export ").unwrap_or(line);

        // Find the `=` separator
        let Some(eq_pos) = line.find('=') else {
            return Err(ZuulError::Validation(format!(
                "Line {}: invalid format, expected KEY=VALUE",
                i + 1
            )));
        };

        let key = line[..eq_pos].trim().to_string();
        let raw_value = &line[eq_pos + 1..];
        let trimmed = raw_value.trim();

        if key.is_empty() {
            return Err(ZuulError::Validation(format!("Line {}: empty key", i + 1)));
        }

        // Check for multiline quoted value: starts with quote but doesn't close on this line
        let quote_char = trimmed.chars().next();
        if matches!(quote_char, Some('"') | Some('\''))
            && !is_closed_quote(trimmed, quote_char.unwrap())
        {
            let q = quote_char.unwrap();
            let start_line = i;
            let mut parts = vec![raw_value];
            i += 1;
            while i < lines.len() {
                parts.push(lines[i]);
                if lines[i].ends_with(q) {
                    break;
                }
                i += 1;
            }
            if i >= lines.len() {
                return Err(ZuulError::Validation(format!(
                    "Line {}: unterminated quoted value",
                    start_line + 1
                )));
            }
            let joined = parts.join("\n");
            let value = unquote(joined.trim());
            secrets.push((key, value));
        } else {
            let value = unquote(trimmed);
            secrets.push((key, value));
        }

        i += 1;
    }

    Ok(secrets)
}

/// Check whether a trimmed value that starts with `q` also closes with `q`.
///
/// A value like `"hello"` is closed; `"hello` is not; `""` is closed (empty quoted).
fn is_closed_quote(s: &str, q: char) -> bool {
    s.len() >= 2 && s.ends_with(q)
}

/// Remove surrounding quotes and unescape common escape sequences.
fn unquote(s: &str) -> String {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        let inner = &s[1..s.len() - 1];
        if s.starts_with('"') {
            return unescape_double_quoted(inner);
        }
        // Single-quoted: literal, no escaping
        return inner.to_string();
    }
    s.to_string()
}

/// Unescape common escape sequences in double-quoted strings.
fn unescape_double_quoted(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('$') => result.push('$'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Parse JSON format (`{"KEY": "value", ...}`).
fn parse_json(content: &str) -> Result<Vec<(String, String)>, ZuulError> {
    let map: HashMap<String, serde_json::Value> = serde_json::from_str(content)
        .map_err(|e| ZuulError::Validation(format!("Invalid JSON: {e}")))?;

    let mut secrets: Vec<(String, String)> = map
        .into_iter()
        .map(|(k, v)| {
            let value = match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            };
            (k, value)
        })
        .collect();

    secrets.sort_by(|(a, _), (b, _)| a.cmp(b));
    Ok(secrets)
}

/// Parse YAML format.
fn parse_yaml(content: &str) -> Result<Vec<(String, String)>, ZuulError> {
    let map: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(content)
        .map_err(|e| ZuulError::Validation(format!("Invalid YAML: {e}")))?;

    let mut secrets: Vec<(String, String)> = map
        .into_iter()
        .map(|(k, v)| {
            let value = match v {
                serde_yaml::Value::String(s) => s,
                other => {
                    // Use serde_yaml to serialize non-string values
                    serde_yaml::to_string(&other)
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                }
            };
            (k, value)
        })
        .collect();

    secrets.sort_by(|(a, _), (b, _)| a.cmp(b));
    Ok(secrets)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- dotenv parser ---

    #[test]
    fn dotenv_basic() {
        let input = "KEY=value\nDB_URL=postgres://localhost";
        let result = parse_dotenv(input).unwrap();
        assert_eq!(
            result,
            vec![
                ("KEY".to_string(), "value".to_string()),
                ("DB_URL".to_string(), "postgres://localhost".to_string()),
            ]
        );
    }

    #[test]
    fn dotenv_double_quoted() {
        let input = r#"KEY="hello world""#;
        let result = parse_dotenv(input).unwrap();
        assert_eq!(result, vec![("KEY".to_string(), "hello world".to_string())]);
    }

    #[test]
    fn dotenv_single_quoted() {
        let input = "KEY='hello world'";
        let result = parse_dotenv(input).unwrap();
        assert_eq!(result, vec![("KEY".to_string(), "hello world".to_string())]);
    }

    #[test]
    fn dotenv_escape_sequences() {
        let input = r#"KEY="line1\nline2\ttab\\slash\"quote""#;
        let result = parse_dotenv(input).unwrap();
        assert_eq!(
            result,
            vec![(
                "KEY".to_string(),
                "line1\nline2\ttab\\slash\"quote".to_string()
            )]
        );
    }

    #[test]
    fn dotenv_comments_and_blanks() {
        let input = "# comment\n\nKEY=value\n  # indented comment\n";
        let result = parse_dotenv(input).unwrap();
        assert_eq!(result, vec![("KEY".to_string(), "value".to_string())]);
    }

    #[test]
    fn dotenv_export_prefix() {
        let input = "export KEY=value\nexport OTHER=\"quoted\"";
        let result = parse_dotenv(input).unwrap();
        assert_eq!(
            result,
            vec![
                ("KEY".to_string(), "value".to_string()),
                ("OTHER".to_string(), "quoted".to_string()),
            ]
        );
    }

    #[test]
    fn dotenv_empty_value() {
        let input = "KEY=";
        let result = parse_dotenv(input).unwrap();
        assert_eq!(result, vec![("KEY".to_string(), String::new())]);
    }

    #[test]
    fn dotenv_empty_quoted_value() {
        let input = "KEY=\"\"";
        let result = parse_dotenv(input).unwrap();
        assert_eq!(result, vec![("KEY".to_string(), String::new())]);
    }

    #[test]
    fn dotenv_invalid_line() {
        let input = "no_equals_sign";
        let result = parse_dotenv(input);
        assert!(result.is_err());
    }

    #[test]
    fn dotenv_value_with_equals() {
        let input = "KEY=a=b=c";
        let result = parse_dotenv(input).unwrap();
        assert_eq!(result, vec![("KEY".to_string(), "a=b=c".to_string())]);
    }

    #[test]
    fn dotenv_multiline_single_quoted() {
        let input = "CONFIG='{\n  \"key\": \"value\"\n}'";
        let result = parse_dotenv(input).unwrap();
        assert_eq!(
            result,
            vec![(
                "CONFIG".to_string(),
                "{\n  \"key\": \"value\"\n}".to_string()
            )]
        );
    }

    #[test]
    fn dotenv_multiline_double_quoted() {
        let input = "CONFIG=\"line1\nline2\nline3\"";
        let result = parse_dotenv(input).unwrap();
        assert_eq!(
            result,
            vec![("CONFIG".to_string(), "line1\nline2\nline3".to_string())]
        );
    }

    #[test]
    fn dotenv_multiline_with_other_keys() {
        let input = "A=1\nJSON='{\n  \"x\": 1\n}'\nB=2";
        let result = parse_dotenv(input).unwrap();
        assert_eq!(
            result,
            vec![
                ("A".to_string(), "1".to_string()),
                ("JSON".to_string(), "{\n  \"x\": 1\n}".to_string()),
                ("B".to_string(), "2".to_string()),
            ]
        );
    }

    #[test]
    fn dotenv_multiline_unterminated() {
        let input = "KEY='unclosed\nvalue";
        let result = parse_dotenv(input);
        assert!(result.is_err());
    }

    // --- JSON parser ---

    #[test]
    fn json_basic() {
        let input = r#"{"API_KEY": "sk_test", "DB_URL": "postgres://localhost"}"#;
        let result = parse_json(input).unwrap();
        assert_eq!(
            result,
            vec![
                ("API_KEY".to_string(), "sk_test".to_string()),
                ("DB_URL".to_string(), "postgres://localhost".to_string()),
            ]
        );
    }

    #[test]
    fn json_non_string_values() {
        let input = r#"{"PORT": 8080, "DEBUG": true}"#;
        let result = parse_json(input).unwrap();
        assert_eq!(
            result,
            vec![
                ("DEBUG".to_string(), "true".to_string()),
                ("PORT".to_string(), "8080".to_string()),
            ]
        );
    }

    #[test]
    fn json_empty() {
        let input = "{}";
        let result = parse_json(input).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn json_invalid() {
        let result = parse_json("not json");
        assert!(result.is_err());
    }

    // --- YAML parser ---

    #[test]
    fn yaml_basic() {
        let input = "API_KEY: sk_test\nDB_URL: postgres://localhost";
        let result = parse_yaml(input).unwrap();
        assert_eq!(
            result,
            vec![
                ("API_KEY".to_string(), "sk_test".to_string()),
                ("DB_URL".to_string(), "postgres://localhost".to_string()),
            ]
        );
    }

    #[test]
    fn yaml_quoted_values() {
        let input = "KEY: \"hello world\"\nOTHER: 'single quoted'";
        let result = parse_yaml(input).unwrap();
        assert_eq!(
            result,
            vec![
                ("KEY".to_string(), "hello world".to_string()),
                ("OTHER".to_string(), "single quoted".to_string()),
            ]
        );
    }

    #[test]
    fn yaml_non_string_values() {
        let input = "PORT: 8080\nDEBUG: true";
        let result = parse_yaml(input).unwrap();
        assert_eq!(
            result,
            vec![
                ("DEBUG".to_string(), "true".to_string()),
                ("PORT".to_string(), "8080".to_string()),
            ]
        );
    }

    #[test]
    fn yaml_empty() {
        // serde_yaml parses empty content as Null, not a mapping
        let input = "{}";
        let result = parse_yaml(input).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn yaml_invalid() {
        let result = parse_yaml(":\n  :\n    - bad");
        assert!(result.is_err());
    }

    // --- format detection ---

    #[test]
    fn detect_json_extension() {
        let f = detect_format(Path::new("secrets.json")).unwrap();
        assert!(matches!(f, ImportFormat::Json));
    }

    #[test]
    fn detect_yaml_extension() {
        let f = detect_format(Path::new("secrets.yaml")).unwrap();
        assert!(matches!(f, ImportFormat::Yaml));
    }

    #[test]
    fn detect_yml_extension() {
        let f = detect_format(Path::new("secrets.yml")).unwrap();
        assert!(matches!(f, ImportFormat::Yaml));
    }

    #[test]
    fn detect_dotenv_default() {
        let f = detect_format(Path::new(".env")).unwrap();
        assert!(matches!(f, ImportFormat::Dotenv));
    }

    #[test]
    fn detect_no_extension() {
        let f = detect_format(Path::new("envfile")).unwrap();
        assert!(matches!(f, ImportFormat::Dotenv));
    }
}
