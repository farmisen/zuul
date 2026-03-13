use std::fs;
use std::io::Write;
use std::path::Path;

use dialoguer::Input;

use crate::error::ZuulError;

/// The config file name.
const CONFIG_FILE: &str = ".zuul.toml";
/// The local overrides file name.
const LOCAL_CONFIG_FILE: &str = ".zuul.local.toml";
/// The gitignore file name.
const GITIGNORE_FILE: &str = ".gitignore";

/// Run the `zuul init` command.
///
/// Creates a `.zuul.toml` in the current directory and ensures
/// `.zuul.local.toml` is listed in `.gitignore`.
pub fn run(dir: &Path, project: Option<String>, backend: &str) -> Result<(), ZuulError> {
    let config_path = dir.join(CONFIG_FILE);

    if config_path.exists() {
        return Err(ZuulError::Config(format!(
            "{CONFIG_FILE} already exists in {}. Remove it first to re-initialize.",
            dir.display()
        )));
    }

    let project_id = match project {
        Some(id) => id,
        None => Input::new()
            .with_prompt("GCP project ID")
            .interact_text()
            .map_err(|e| ZuulError::Config(format!("Failed to read input: {e}")))?,
    };

    let config_content = format!(
        "[backend]\n\
         type = \"{backend}\"\n\
         project_id = \"{project_id}\"\n\
         \n\
         [defaults]\n\
         environment = \"dev\"\n"
    );

    fs::write(&config_path, &config_content)
        .map_err(|e| ZuulError::Config(format!("Failed to write {CONFIG_FILE}: {e}")))?;

    add_to_gitignore(dir)?;

    println!("Created {CONFIG_FILE} in {}", dir.display());
    println!("\nNext steps:");
    println!("  zuul auth          # Set up GCP authentication");
    println!("  zuul env create    # Create your first environment");

    Ok(())
}

/// Ensure `.zuul.local.toml` is listed in `.gitignore`.
///
/// Creates the file if it doesn't exist, or appends to it if the
/// entry is not already present.
fn add_to_gitignore(dir: &Path) -> Result<(), ZuulError> {
    let gitignore_path = dir.join(GITIGNORE_FILE);

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)
            .map_err(|e| ZuulError::Config(format!("Failed to read {GITIGNORE_FILE}: {e}")))?;

        if content.lines().any(|line| line.trim() == LOCAL_CONFIG_FILE) {
            return Ok(());
        }

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&gitignore_path)
            .map_err(|e| ZuulError::Config(format!("Failed to open {GITIGNORE_FILE}: {e}")))?;

        // Ensure we start on a new line
        if !content.ends_with('\n') && !content.is_empty() {
            writeln!(file)
                .map_err(|e| ZuulError::Config(format!("Failed to write {GITIGNORE_FILE}: {e}")))?;
        }

        writeln!(file, "{LOCAL_CONFIG_FILE}")
            .map_err(|e| ZuulError::Config(format!("Failed to write {GITIGNORE_FILE}: {e}")))?;
    } else {
        fs::write(&gitignore_path, format!("{LOCAL_CONFIG_FILE}\n"))
            .map_err(|e| ZuulError::Config(format!("Failed to create {GITIGNORE_FILE}: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn init_creates_config_file() {
        let dir = tempfile::tempdir().unwrap();
        run(
            dir.path(),
            Some("my-project".to_string()),
            "gcp-secret-manager",
        )
        .unwrap();

        let content = fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
        assert!(content.contains("project_id = \"my-project\""));
        assert!(content.contains("type = \"gcp-secret-manager\""));
        assert!(content.contains("environment = \"dev\""));
    }

    #[test]
    fn init_fails_if_config_exists() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(CONFIG_FILE), "existing").unwrap();

        let result = run(
            dir.path(),
            Some("my-project".to_string()),
            "gcp-secret-manager",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn init_creates_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        run(
            dir.path(),
            Some("my-project".to_string()),
            "gcp-secret-manager",
        )
        .unwrap();

        let content = fs::read_to_string(dir.path().join(GITIGNORE_FILE)).unwrap();
        assert!(content.contains(LOCAL_CONFIG_FILE));
    }

    #[test]
    fn init_appends_to_existing_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(GITIGNORE_FILE), "node_modules/\n").unwrap();

        run(
            dir.path(),
            Some("my-project".to_string()),
            "gcp-secret-manager",
        )
        .unwrap();

        let content = fs::read_to_string(dir.path().join(GITIGNORE_FILE)).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains(LOCAL_CONFIG_FILE));
    }

    #[test]
    fn init_appends_newline_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(GITIGNORE_FILE), "node_modules/").unwrap(); // no trailing newline

        run(
            dir.path(),
            Some("my-project".to_string()),
            "gcp-secret-manager",
        )
        .unwrap();

        let content = fs::read_to_string(dir.path().join(GITIGNORE_FILE)).unwrap();
        assert!(content.contains("node_modules/\n"));
        assert!(content.contains(LOCAL_CONFIG_FILE));
    }

    #[test]
    fn init_skips_gitignore_if_already_present() {
        let dir = tempfile::tempdir().unwrap();
        let initial = format!("node_modules/\n{LOCAL_CONFIG_FILE}\n");
        fs::write(dir.path().join(GITIGNORE_FILE), &initial).unwrap();

        run(
            dir.path(),
            Some("my-project".to_string()),
            "gcp-secret-manager",
        )
        .unwrap();

        let content = fs::read_to_string(dir.path().join(GITIGNORE_FILE)).unwrap();
        assert_eq!(content, initial); // unchanged
    }
}
