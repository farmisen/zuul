use std::fs;
use std::io::Write;
use std::path::Path;

use console::style;

use crate::backend::file_backend::DEFAULT_STORE_FILE;
use crate::error::ZuulError;
use crate::prompt;

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
/// For the `file` backend, also creates an empty encrypted store.
pub fn run(
    dir: &Path,
    project: Option<String>,
    backend: &str,
    non_interactive: bool,
) -> Result<(), ZuulError> {
    let config_path = dir.join(CONFIG_FILE);

    if config_path.exists() {
        return Err(ZuulError::Config(format!(
            "{CONFIG_FILE} already exists in {}. Remove it first to re-initialize.",
            dir.display()
        )));
    }

    match backend {
        "file" => init_file_backend(dir, &config_path, non_interactive)?,
        _ => init_gcp_backend(dir, &config_path, project, backend, non_interactive)?,
    }

    add_to_gitignore(dir, LOCAL_CONFIG_FILE)?;

    println!(
        "{} Created {CONFIG_FILE} in {}",
        style("✔").green(),
        dir.display()
    );

    Ok(())
}

/// Initialize the GCP Secret Manager backend.
fn init_gcp_backend(
    _dir: &Path,
    config_path: &Path,
    project: Option<String>,
    backend: &str,
    non_interactive: bool,
) -> Result<(), ZuulError> {
    let project_id = match project {
        Some(id) => id,
        None => prompt::input("GCP project ID", non_interactive)?,
    };

    let config_content = format!(
        "[backend]\n\
         type = \"{backend}\"\n\
         project_id = \"{project_id}\"\n\
         \n\
         [defaults]\n\
         environment = \"dev\"\n"
    );

    fs::write(config_path, &config_content)
        .map_err(|e| ZuulError::Config(format!("Failed to write {CONFIG_FILE}: {e}")))?;

    println!("\nNext steps:");
    println!("  cd terraform && terraform apply   # Provision environments and IAM");
    println!("  zuul auth                         # Set up GCP authentication");

    Ok(())
}

/// Initialize the file-based encrypted backend.
fn init_file_backend(
    dir: &Path,
    config_path: &Path,
    non_interactive: bool,
) -> Result<(), ZuulError> {
    let passphrase = match std::env::var("ZUUL_PASSPHRASE") {
        Ok(p) => p,
        Err(_) => prompt::password(
            "Choose a passphrase for encrypting secrets",
            non_interactive,
        )?,
    };

    let config_content = format!(
        "[backend]\n\
         type = \"file\"\n\
         # path = \"{DEFAULT_STORE_FILE}\"    # default\n\
         \n\
         [defaults]\n\
         environment = \"dev\"\n"
    );

    fs::write(config_path, &config_content)
        .map_err(|e| ZuulError::Config(format!("Failed to write {CONFIG_FILE}: {e}")))?;

    // Create the empty encrypted store.
    let store_path = dir.join(DEFAULT_STORE_FILE);
    create_empty_store(&store_path, &passphrase)?;

    // Add the encrypted store to .gitignore (it contains secrets).
    add_to_gitignore(dir, DEFAULT_STORE_FILE)?;

    println!("\nNext steps:");
    println!("  zuul env create dev               # Create your first environment");
    println!("  zuul secret set KEY --env dev      # Set a secret");

    Ok(())
}

/// Create an empty encrypted store file.
fn create_empty_store(store_path: &Path, passphrase: &str) -> Result<(), ZuulError> {
    use std::io::Write as _;

    let empty_store = serde_json::json!({
        "version": 1,
        "environments": {},
        "secrets": {},
        "metadata": {}
    });
    let plaintext = serde_json::to_vec_pretty(&empty_store)
        .map_err(|e| ZuulError::Backend(format!("Failed to serialize empty store: {e}")))?;

    let encryptor = age::Encryptor::with_user_passphrase(age::secrecy::SecretString::new(
        passphrase.to_string(),
    ));
    let mut ciphertext = Vec::new();
    let mut writer = encryptor
        .wrap_output(&mut ciphertext)
        .map_err(|e| ZuulError::Backend(format!("Failed to initialize encryption: {e}")))?;
    writer
        .write_all(&plaintext)
        .map_err(|e| ZuulError::Backend(format!("Failed to encrypt: {e}")))?;
    writer
        .finish()
        .map_err(|e| ZuulError::Backend(format!("Failed to finalize encryption: {e}")))?;

    fs::write(store_path, &ciphertext).map_err(|e| {
        ZuulError::Backend(format!(
            "Failed to write store '{}': {e}",
            store_path.display()
        ))
    })?;

    Ok(())
}

/// Ensure the given entry is listed in `.gitignore`.
///
/// Creates the file if it doesn't exist, or appends to it if the
/// entry is not already present.
fn add_to_gitignore(dir: &Path, entry: &str) -> Result<(), ZuulError> {
    let gitignore_path = dir.join(GITIGNORE_FILE);

    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)
            .map_err(|e| ZuulError::Config(format!("Failed to read {GITIGNORE_FILE}: {e}")))?;

        if content.lines().any(|line| line.trim() == entry) {
            return Ok(());
        }

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&gitignore_path)
            .map_err(|e| ZuulError::Config(format!("Failed to open {GITIGNORE_FILE}: {e}")))?;

        if !content.ends_with('\n') && !content.is_empty() {
            writeln!(file)
                .map_err(|e| ZuulError::Config(format!("Failed to write {GITIGNORE_FILE}: {e}")))?;
        }

        writeln!(file, "{entry}")
            .map_err(|e| ZuulError::Config(format!("Failed to write {GITIGNORE_FILE}: {e}")))?;
    } else {
        fs::write(&gitignore_path, format!("{entry}\n"))
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
            true,
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
            true,
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
            true,
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
            true,
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
            true,
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
            true,
        )
        .unwrap();

        let content = fs::read_to_string(dir.path().join(GITIGNORE_FILE)).unwrap();
        assert_eq!(content, initial); // unchanged
    }
}
