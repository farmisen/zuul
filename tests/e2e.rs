//! End-to-end integration tests against the GCP Secret Manager emulator.
//!
//! These tests require a running emulator on `localhost:9090`.
//! Start it with: `docker compose -f docker-compose.emulator.yml up -d`
//!
//! Run with: `cargo test --test e2e -- --ignored`

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

/// Build the zuul binary once (shared across all tests) and return the path.
fn zuul_bin() -> &'static str {
    static BIN_PATH: OnceLock<String> = OnceLock::new();
    BIN_PATH.get_or_init(|| {
        let output = Command::new("cargo")
            .args(["build", "--quiet"])
            .output()
            .expect("failed to build zuul");
        assert!(
            output.status.success(),
            "cargo build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let metadata = Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .output()
            .expect("failed to get cargo metadata");
        let meta: serde_json::Value =
            serde_json::from_slice(&metadata.stdout).expect("failed to parse metadata");
        let target_dir = meta["target_directory"].as_str().unwrap();
        format!("{target_dir}/debug/zuul")
    })
}

/// Run a zuul command in a given working directory with emulator env set.
fn zuul(bin: &str, work_dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(bin)
        .args(args)
        .current_dir(work_dir)
        .env("SECRET_MANAGER_EMULATOR_HOST", "http://localhost:9090")
        .output()
        .unwrap_or_else(|e| panic!("failed to run zuul {}: {e}", args.join(" ")))
}

/// Run a zuul command and assert it succeeded, returning stdout.
fn zuul_ok(bin: &str, work_dir: &Path, args: &[&str]) -> String {
    let output = zuul(bin, work_dir, args);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "zuul {} failed (exit {}):\nstdout: {stdout}\nstderr: {stderr}",
        args.join(" "),
        output.status.code().unwrap_or(-1),
    );
    stdout
}

/// Run a zuul command and assert it failed, returning stderr.
fn zuul_err(bin: &str, work_dir: &Path, args: &[&str]) -> String {
    let output = zuul(bin, work_dir, args);
    assert!(
        !output.status.success(),
        "zuul {} should have failed but succeeded:\nstdout: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
    );
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Create a temp dir with a `.zuul.toml` pointing at a test project.
fn setup_project(project_id: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let config = format!(
        r#"[backend]
type = "gcp-secret-manager"
project_id = "{project_id}"

[defaults]
environment = "dev"
"#
    );
    std::fs::write(dir.path().join(".zuul.toml"), config).expect("failed to write .zuul.toml");
    dir
}

#[test]
#[ignore]
fn e2e_init_creates_config() {
    let bin = zuul_bin();
    let dir = tempfile::tempdir().unwrap();

    let stdout = zuul_ok(&bin, dir.path(), &["init", "--project", "e2e-test-project"]);
    assert!(stdout.contains("Created"));
    assert!(dir.path().join(".zuul.toml").exists());

    let config = std::fs::read_to_string(dir.path().join(".zuul.toml")).unwrap();
    assert!(config.contains("e2e-test-project"));
}

#[test]
#[ignore]
fn e2e_environment_lifecycle() {
    let bin = zuul_bin();
    let dir = setup_project("e2e-env-lifecycle");

    // Create environments
    zuul_ok(
        &bin,
        dir.path(),
        &["env", "create", "dev", "--description", "Development"],
    );
    zuul_ok(
        &bin,
        dir.path(),
        &["env", "create", "staging", "--description", "Staging"],
    );

    // List environments — should show both
    let stdout = zuul_ok(&bin, dir.path(), &["env", "list"]);
    assert!(stdout.contains("dev"), "env list should contain 'dev'");
    assert!(
        stdout.contains("staging"),
        "env list should contain 'staging'"
    );

    // Show environment details
    let stdout = zuul_ok(&bin, dir.path(), &["env", "show", "dev"]);
    assert!(stdout.contains("dev"), "env show should contain 'dev'");
    assert!(
        stdout.contains("Development"),
        "env show should contain description"
    );

    // Update environment description
    zuul_ok(
        &bin,
        dir.path(),
        &["env", "update", "dev", "--description", "Local development"],
    );
    let stdout = zuul_ok(&bin, dir.path(), &["env", "show", "dev"]);
    assert!(
        stdout.contains("Local development"),
        "description should be updated"
    );

    // List with JSON format
    let stdout = zuul_ok(&bin, dir.path(), &["--format", "json", "env", "list"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 2);

    // Delete dry-run should not actually delete
    zuul_ok(&bin, dir.path(), &["env", "delete", "staging", "--dry-run"]);
    let stdout = zuul_ok(&bin, dir.path(), &["env", "list"]);
    assert!(
        stdout.contains("staging"),
        "staging should still exist after dry-run"
    );
}

#[test]
#[ignore]
fn e2e_secret_crud() {
    let bin = zuul_bin();
    let dir = setup_project("e2e-secret-crud");

    // Create an environment first
    zuul_ok(&bin, dir.path(), &["env", "create", "dev"]);

    // Set a secret
    zuul_ok(
        &bin,
        dir.path(),
        &[
            "-e",
            "dev",
            "secret",
            "set",
            "DATABASE_URL",
            "postgres://localhost:5432/mydb",
        ],
    );

    // Get the secret
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "secret", "get", "DATABASE_URL"],
    );
    assert_eq!(stdout.trim(), "postgres://localhost:5432/mydb");

    // Set another secret
    zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "secret", "set", "API_KEY", "sk_test_12345"],
    );

    // List secrets
    let stdout = zuul_ok(&bin, dir.path(), &["-e", "dev", "secret", "list"]);
    assert!(stdout.contains("DATABASE_URL"));
    assert!(stdout.contains("API_KEY"));

    // Secret info
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "secret", "info", "DATABASE_URL"],
    );
    assert!(stdout.contains("DATABASE_URL"));

    // Update a secret
    zuul_ok(
        &bin,
        dir.path(),
        &[
            "-e",
            "dev",
            "secret",
            "set",
            "DATABASE_URL",
            "postgres://localhost:5432/newdb",
        ],
    );
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "secret", "get", "DATABASE_URL"],
    );
    assert_eq!(stdout.trim(), "postgres://localhost:5432/newdb");

    // Delete secret (with --force to skip confirmation)
    zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "secret", "delete", "API_KEY", "--force"],
    );
    let stdout = zuul_ok(&bin, dir.path(), &["-e", "dev", "secret", "list"]);
    assert!(!stdout.contains("API_KEY"), "API_KEY should be deleted");
    assert!(
        stdout.contains("DATABASE_URL"),
        "DATABASE_URL should remain"
    );
}

#[test]
#[ignore]
fn e2e_secret_copy() {
    let bin = zuul_bin();
    let dir = setup_project("e2e-secret-copy");

    // Create two environments
    zuul_ok(&bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(&bin, dir.path(), &["env", "create", "staging"]);

    // Set a secret in dev
    zuul_ok(
        &bin,
        dir.path(),
        &[
            "-e",
            "dev",
            "secret",
            "set",
            "DB_URL",
            "postgres://dev:5432/db",
        ],
    );

    // Copy to staging
    zuul_ok(
        &bin,
        dir.path(),
        &[
            "secret", "copy", "DB_URL", "--from", "dev", "--to", "staging",
        ],
    );

    // Verify it exists in staging
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "staging", "secret", "get", "DB_URL"],
    );
    assert_eq!(stdout.trim(), "postgres://dev:5432/db");
}

#[test]
#[ignore]
fn e2e_export_formats() {
    let bin = zuul_bin();
    let dir = setup_project("e2e-export");

    // Setup
    zuul_ok(&bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        &bin,
        dir.path(),
        &[
            "-e",
            "dev",
            "secret",
            "set",
            "DB_URL",
            "postgres://localhost/db",
        ],
    );
    zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "secret", "set", "API_KEY", "secret123"],
    );

    // Export dotenv
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "export", "--export-format", "dotenv"],
    );
    assert!(stdout.contains("DB_URL="));
    assert!(stdout.contains("API_KEY="));

    // Export JSON
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "export", "--export-format", "json"],
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    assert_eq!(
        parsed["DB_URL"].as_str().unwrap(),
        "postgres://localhost/db"
    );
    assert_eq!(parsed["API_KEY"].as_str().unwrap(), "secret123");

    // Export YAML
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "export", "--export-format", "yaml"],
    );
    assert!(stdout.contains("DB_URL:"));
    assert!(stdout.contains("API_KEY:"));

    // Export to file
    let output_file = dir.path().join("exported.env");
    zuul_ok(
        &bin,
        dir.path(),
        &[
            "-e",
            "dev",
            "export",
            "--export-format",
            "dotenv",
            "--output",
            output_file.to_str().unwrap(),
        ],
    );
    assert!(output_file.exists());
    let content = std::fs::read_to_string(&output_file).unwrap();
    assert!(content.contains("DB_URL="));
}

#[test]
#[ignore]
fn e2e_run_injects_secrets() {
    let bin = zuul_bin();
    let dir = setup_project("e2e-run");

    // Setup
    zuul_ok(&bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "secret", "set", "MY_VAR", "hello_zuul"],
    );

    // Run a command that prints the env var
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "run", "--", "sh", "-c", "echo $MY_VAR"],
    );
    assert_eq!(stdout.trim(), "hello_zuul");
}

#[test]
#[ignore]
fn e2e_run_forwards_exit_code() {
    let bin = zuul_bin();
    let dir = setup_project("e2e-run-exit");

    // Setup
    zuul_ok(&bin, dir.path(), &["env", "create", "dev"]);

    // Run a command that exits with code 42
    let output = zuul(
        &bin,
        dir.path(),
        &["-e", "dev", "run", "--", "sh", "-c", "exit 42"],
    );
    assert_eq!(output.status.code(), Some(42));
}

#[test]
#[ignore]
fn e2e_import_dotenv() {
    let bin = zuul_bin();
    let dir = setup_project("e2e-import");

    // Setup
    zuul_ok(&bin, dir.path(), &["env", "create", "dev"]);

    // Create a .env file to import
    let env_file = dir.path().join("test.env");
    std::fs::write(
        &env_file,
        r#"# Test env file
DB_HOST=localhost
DB_PORT=5432
DB_NAME=mydb
"#,
    )
    .unwrap();

    // Dry run first
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &[
            "-e",
            "dev",
            "import",
            "--file",
            env_file.to_str().unwrap(),
            "--dry-run",
        ],
    );
    assert!(stdout.contains("DB_HOST") || stdout.contains("dry"));

    // Actually import
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "import", "--file", env_file.to_str().unwrap()],
    );
    assert!(stdout.contains("Imported") || stdout.contains("3"));

    // Verify secrets were created
    let stdout = zuul_ok(&bin, dir.path(), &["-e", "dev", "secret", "get", "DB_HOST"]);
    assert_eq!(stdout.trim(), "localhost");

    let stdout = zuul_ok(&bin, dir.path(), &["-e", "dev", "secret", "get", "DB_PORT"]);
    assert_eq!(stdout.trim(), "5432");
}

#[test]
#[ignore]
fn e2e_import_json() {
    let bin = zuul_bin();
    let dir = setup_project("e2e-import-json");

    zuul_ok(&bin, dir.path(), &["env", "create", "dev"]);

    let json_file = dir.path().join("secrets.json");
    std::fs::write(
        &json_file,
        r#"{"REDIS_URL": "redis://localhost:6379", "CACHE_TTL": "3600"}"#,
    )
    .unwrap();

    zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "import", "--file", json_file.to_str().unwrap()],
    );

    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "secret", "get", "REDIS_URL"],
    );
    assert_eq!(stdout.trim(), "redis://localhost:6379");
}

#[test]
#[ignore]
fn e2e_local_overrides() {
    let bin = zuul_bin();
    let dir = setup_project("e2e-overrides");

    zuul_ok(&bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        &bin,
        dir.path(),
        &[
            "-e",
            "dev",
            "secret",
            "set",
            "DB_URL",
            "postgres://remote/db",
        ],
    );

    // Create a local override
    std::fs::write(
        dir.path().join(".zuul.local.toml"),
        r#"[secrets]
DB_URL = "postgres://localhost/localdb"
"#,
    )
    .unwrap();

    // Export with local overrides (default)
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "export", "--export-format", "json"],
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        parsed["DB_URL"].as_str().unwrap(),
        "postgres://localhost/localdb",
        "local override should win"
    );

    // Export without local overrides
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &[
            "-e",
            "dev",
            "export",
            "--export-format",
            "json",
            "--no-local",
        ],
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        parsed["DB_URL"].as_str().unwrap(),
        "postgres://remote/db",
        "--no-local should use backend value"
    );
}

#[test]
#[ignore]
fn e2e_error_missing_environment() {
    let bin = zuul_bin();
    let dir = setup_project("e2e-errors");

    // Try to set a secret in a non-existent environment
    let stderr = zuul_err(
        &bin,
        dir.path(),
        &["-e", "nonexistent", "secret", "set", "KEY", "value"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("exist") || stderr.contains("found"),
        "should report environment not found, got: {stderr}"
    );
}

#[test]
#[ignore]
fn e2e_metadata_operations() {
    let bin = zuul_bin();
    let dir = setup_project("e2e-metadata");

    zuul_ok(&bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "secret", "set", "API_KEY", "test123"],
    );

    // Set metadata
    zuul_ok(
        &bin,
        dir.path(),
        &[
            "-e",
            "dev",
            "secret",
            "metadata",
            "set",
            "API_KEY",
            "owner",
            "backend-team",
        ],
    );

    // List metadata
    let stdout = zuul_ok(
        &bin,
        dir.path(),
        &["-e", "dev", "secret", "metadata", "list", "API_KEY"],
    );
    assert!(stdout.contains("owner"));
    assert!(stdout.contains("backend-team"));

    // Delete metadata — verify the command succeeds.
    // Note: the emulator does not persist annotation removals, so we only
    // assert that the delete command itself exits successfully.
    zuul_ok(
        &bin,
        dir.path(),
        &[
            "-e", "dev", "secret", "metadata", "delete", "API_KEY", "owner",
        ],
    );
}
