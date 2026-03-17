//! CLI-level init tests for the file backend.

use crate::helpers::*;

#[test]
fn init_file_backend_creates_config_and_store() {
    let dir = tempfile::tempdir().unwrap();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["init", "--backend", "file"]);

    // Config file created
    let config = std::fs::read_to_string(dir.path().join(".zuul.toml")).unwrap();
    assert!(config.contains("type = \"file\""));

    // Encrypted store created
    assert!(
        dir.path().join(".zuul.secrets.enc").exists(),
        "encrypted store should be created"
    );

    // .gitignore updated
    let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains(".zuul.local.toml"));
    assert!(gitignore.contains(".zuul.secrets.enc"));
}

#[test]
fn init_file_backend_fails_if_config_exists() {
    let dir = tempfile::tempdir().unwrap();
    let bin = zuul_bin();

    std::fs::write(dir.path().join(".zuul.toml"), "[backend]\n").unwrap();

    let stderr = zuul_err(bin, dir.path(), &["init", "--backend", "file"]);
    assert!(
        stderr.contains("already exists"),
        "should refuse re-init, got: {stderr}"
    );
}

#[test]
fn init_file_backend_then_create_env_and_set_secret() {
    let dir = tempfile::tempdir().unwrap();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["init", "--backend", "file"]);
    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "dev", "value"],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "KEY", "--env", "dev"]);
    assert_eq!(stdout.trim(), "value");
}
