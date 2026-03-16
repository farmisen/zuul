use crate::helpers::*;

#[test]
#[ignore]
fn init_creates_config_and_gitignore() {
    let bin = zuul_bin();
    let dir = tempfile::tempdir().unwrap();

    let stdout = zuul_ok(bin, dir.path(), &["init", "--project", "my-gcp-project"]);
    assert!(
        stdout.contains("Created") || stdout.contains("Initialized"),
        "should confirm creation, got: {stdout}"
    );

    // .zuul.toml should exist with the project ID
    let config_path = dir.path().join(".zuul.toml");
    assert!(config_path.exists(), ".zuul.toml should be created");
    let config = std::fs::read_to_string(&config_path).unwrap();
    assert!(config.contains("my-gcp-project"));
    assert!(config.contains("gcp-secret-manager"));

    // .gitignore should contain .zuul.local.toml
    let gitignore_path = dir.path().join(".gitignore");
    assert!(gitignore_path.exists(), ".gitignore should be created");
    let gitignore = std::fs::read_to_string(&gitignore_path).unwrap();
    assert!(
        gitignore.contains(".zuul.local.toml"),
        ".gitignore should contain .zuul.local.toml, got: {gitignore}"
    );
}

#[test]
#[ignore]
fn init_fails_if_config_exists() {
    let bin = zuul_bin();
    let dir = tempfile::tempdir().unwrap();

    // Create an existing .zuul.toml
    std::fs::write(dir.path().join(".zuul.toml"), "existing").unwrap();

    let stderr = zuul_err(bin, dir.path(), &["init", "--project", "test"]);
    assert!(
        stderr.contains("already exists"),
        "should refuse to overwrite, got: {stderr}"
    );
}

#[test]
#[ignore]
fn init_appends_to_existing_gitignore() {
    let bin = zuul_bin();
    let dir = tempfile::tempdir().unwrap();

    // Create a pre-existing .gitignore
    std::fs::write(dir.path().join(".gitignore"), "node_modules/\n").unwrap();

    zuul_ok(bin, dir.path(), &["init", "--project", "test-proj"]);

    let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(
        gitignore.contains("node_modules/"),
        "existing entries should be preserved"
    );
    assert!(
        gitignore.contains(".zuul.local.toml"),
        "should append .zuul.local.toml"
    );
}

#[test]
#[ignore]
fn init_skips_gitignore_if_already_listed() {
    let bin = zuul_bin();
    let dir = tempfile::tempdir().unwrap();

    // .gitignore already contains the entry
    std::fs::write(
        dir.path().join(".gitignore"),
        "node_modules/\n.zuul.local.toml\n",
    )
    .unwrap();

    zuul_ok(bin, dir.path(), &["init", "--project", "test-proj"]);

    let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    // Should not duplicate
    assert_eq!(
        gitignore.matches(".zuul.local.toml").count(),
        1,
        "should not duplicate entry"
    );
}

#[test]
#[ignore]
fn init_with_custom_backend() {
    let bin = zuul_bin();
    let dir = tempfile::tempdir().unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "init",
            "--project",
            "proj",
            "--backend",
            "gcp-secret-manager",
        ],
    );

    let config = std::fs::read_to_string(dir.path().join(".zuul.toml")).unwrap();
    assert!(config.contains("gcp-secret-manager"));
}

#[test]
#[ignore]
fn init_without_project_in_non_interactive_fails() {
    let bin = zuul_bin();
    let dir = tempfile::tempdir().unwrap();

    // No --project and non-interactive mode → should fail
    let stderr = zuul_err(bin, dir.path(), &["init"]);
    assert!(
        stderr.contains("Input required") || stderr.contains("project"),
        "should ask for project ID, got: {stderr}"
    );
}
