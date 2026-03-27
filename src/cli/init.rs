use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use console::style;

use age::secrecy::ExposeSecret;

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

/// Default identity file path.
const DEFAULT_IDENTITY_DIR: &str = ".zuul";
const DEFAULT_IDENTITY_FILE: &str = "key.txt";

/// Initialize the file-based encrypted backend.
///
/// Initializes the file-based encrypted backend.
///
/// In non-interactive mode, uses env vars to determine the encryption mode:
/// - `ZUUL_KEY_FILE` → identity file
/// - `ZUUL_PASSPHRASE` → passphrase
///
/// In interactive mode, prompts the user to choose.
fn init_file_backend(
    dir: &Path,
    config_path: &Path,
    non_interactive: bool,
) -> Result<(), ZuulError> {
    // Non-interactive: env vars determine the mode.
    if non_interactive {
        if std::env::var("ZUUL_KEY_FILE").is_ok() {
            // Identity mode via env var
        } else if std::env::var("ZUUL_PASSPHRASE").is_ok() {
            return init_file_backend_passphrase(dir, config_path);
        } else {
            return Err(ZuulError::Validation(
                "Non-interactive mode requires ZUUL_KEY_FILE or ZUUL_PASSPHRASE to be set."
                    .to_string(),
            ));
        }
    } else if std::env::var("ZUUL_KEY_FILE").is_ok() {
        // Explicit env var override — skip prompt
    } else if std::env::var("ZUUL_PASSPHRASE").is_ok() {
        return init_file_backend_passphrase(dir, config_path);
    } else {
        // Interactive: ask the user
        println!("\nHow would you like to secure your secrets?\n");
        println!("  1. Identity file (recommended) — fast, works with direnv");
        println!("  2. Passphrase — portable, no key file to manage, does not work with direnv\n");

        let choice = prompt::input("Choice [1]", non_interactive)?;
        let choice = choice.trim();

        if choice == "2" {
            let passphrase = prompt::password(
                "Choose a passphrase for encrypting secrets",
                non_interactive,
            )?;
            // Temporarily set ZUUL_PASSPHRASE so init_file_backend_passphrase can use it
            return init_file_backend_with_passphrase(dir, config_path, &passphrase);
        }
        // choice == "1" or empty (default) → identity file
    }

    init_file_backend_identity(dir, config_path)
}

/// Initialize the file backend with an age identity file.
///
/// Resolves the identity file path from `ZUUL_KEY_FILE` env var or the
/// default `~/.zuul/key.txt`, generates a new keypair if needed, writes
/// the config file, and creates an empty encrypted store.
fn init_file_backend_identity(dir: &Path, config_path: &Path) -> Result<(), ZuulError> {
    let identity_path = if let Ok(path) = std::env::var("ZUUL_KEY_FILE") {
        PathBuf::from(path)
    } else {
        let home = std::env::var("HOME")
            .map_err(|_| ZuulError::Config("HOME environment variable not set.".to_string()))?;
        PathBuf::from(&home)
            .join(DEFAULT_IDENTITY_DIR)
            .join(DEFAULT_IDENTITY_FILE)
    };

    if identity_path.exists() {
        println!("Using existing identity file: {}", identity_path.display());
    } else {
        generate_identity_file(&identity_path)?;
    }

    let identity_path_str = format!("~/{DEFAULT_IDENTITY_DIR}/{DEFAULT_IDENTITY_FILE}");
    let config_content = format!(
        "[backend]\n\
         type = \"file\"\n\
         # path = \"{DEFAULT_STORE_FILE}\"    # default\n\
         identity = \"{identity_path_str}\"\n\
         \n\
         [defaults]\n\
         environment = \"dev\"\n"
    );

    fs::write(config_path, &config_content)
        .map_err(|e| ZuulError::Config(format!("Failed to write {CONFIG_FILE}: {e}")))?;

    let store_path = dir.join(DEFAULT_STORE_FILE);
    create_empty_store_with_identity(&store_path, &identity_path)?;

    add_to_gitignore(dir, DEFAULT_STORE_FILE)?;

    println!("\nNext steps:");
    println!("  zuul env create dev               # Create your first environment");
    println!("  zuul secret set KEY --env dev      # Set a secret");

    Ok(())
}

/// Generate a new age identity file at the given path.
fn generate_identity_file(identity_path: &Path) -> Result<(), ZuulError> {
    let identity_dir = identity_path
        .parent()
        .ok_or_else(|| ZuulError::Config("Invalid identity file path.".to_string()))?;

    fs::create_dir_all(identity_dir).map_err(|e| {
        ZuulError::Config(format!(
            "Failed to create directory '{}': {e}",
            identity_dir.display()
        ))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(identity_dir, fs::Permissions::from_mode(0o700)).ok();
    }

    let identity = age::x25519::Identity::generate();
    let public_key = identity.to_public();

    let key_content = format!(
        "# created: {}\n# public key: {}\n{}\n",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        public_key,
        identity.to_string().expose_secret()
    );

    fs::write(identity_path, &key_content).map_err(|e| {
        ZuulError::Config(format!(
            "Failed to write identity file '{}': {e}",
            identity_path.display()
        ))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(identity_path, fs::Permissions::from_mode(0o600)).map_err(|e| {
            ZuulError::Config(format!(
                "Failed to set permissions on '{}': {e}",
                identity_path.display()
            ))
        })?;
    }

    println!("Generated identity file: {}", identity_path.display());
    Ok(())
}

/// Initialize the file backend with passphrase from `ZUUL_PASSPHRASE` env var.
fn init_file_backend_passphrase(dir: &Path, config_path: &Path) -> Result<(), ZuulError> {
    let passphrase = std::env::var("ZUUL_PASSPHRASE")
        .map_err(|_| ZuulError::Auth("ZUUL_PASSPHRASE env var not set.".to_string()))?;
    init_file_backend_with_passphrase(dir, config_path, &passphrase)
}

/// Initialize the file backend with a given passphrase.
fn init_file_backend_with_passphrase(
    dir: &Path,
    config_path: &Path,
    passphrase: &str,
) -> Result<(), ZuulError> {
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

    let store_path = dir.join(DEFAULT_STORE_FILE);
    create_empty_store_with_passphrase(&store_path, passphrase)?;

    add_to_gitignore(dir, DEFAULT_STORE_FILE)?;

    println!("\nNext steps:");
    println!("  zuul env create dev               # Create your first environment");
    println!("  zuul secret set KEY --env dev      # Set a secret");

    Ok(())
}

/// Create an empty encrypted store using an age identity file.
fn create_empty_store_with_identity(
    store_path: &Path,
    identity_path: &Path,
) -> Result<(), ZuulError> {
    use std::io::Write as _;

    let contents = fs::read_to_string(identity_path).map_err(|e| {
        ZuulError::Config(format!(
            "Failed to read identity file '{}': {e}",
            identity_path.display()
        ))
    })?;

    let identity: age::x25519::Identity = contents
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .find_map(|line| line.parse().ok())
        .ok_or_else(|| {
            ZuulError::Config(format!(
                "No valid age identity found in '{}'.",
                identity_path.display()
            ))
        })?;

    let recipient = identity.to_public();
    let empty_store = serde_json::json!({
        "version": 1,
        "environments": {},
        "secrets": {},
        "metadata": {}
    });
    let plaintext = serde_json::to_vec_pretty(&empty_store)
        .map_err(|e| ZuulError::Backend(format!("Failed to serialize empty store: {e}")))?;

    let encryptor =
        age::Encryptor::with_recipients(vec![Box::new(recipient)]).expect("at least one recipient");
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

/// Create an empty encrypted store using a passphrase.
fn create_empty_store_with_passphrase(
    store_path: &Path,
    passphrase: &str,
) -> Result<(), ZuulError> {
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
