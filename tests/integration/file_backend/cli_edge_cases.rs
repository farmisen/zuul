//! CLI edge-case and error-path tests for the file backend.
//!
//! Covers: nonexistent environments, validation, missing config, missing
//! default env, and other error conditions.

use crate::helpers::*;

// ---------------------------------------------------------------------------
// No config file
// ---------------------------------------------------------------------------

#[test]
fn no_config_file_fails_with_message() {
    let dir = tempfile::tempdir().unwrap();
    let bin = zuul_bin();

    // No .zuul.toml — should fail with a helpful message.
    let stderr = zuul_err(bin, dir.path(), &["env", "list"]);
    assert!(
        stderr.contains("No GCP project ID")
            || stderr.contains("init")
            || stderr.contains("config"),
        "should suggest running init, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Missing --env and no default
// ---------------------------------------------------------------------------

#[test]
fn missing_env_with_no_default_fails() {
    let dir = setup_project_no_default_env();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "dev", "val"],
    );

    // secret get without --env and no default configured
    let stderr = zuul_err(bin, dir.path(), &["secret", "get", "KEY"]);
    assert!(
        stderr.contains("environment") || stderr.contains("--env"),
        "should mention missing environment, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Operations on nonexistent environment
// ---------------------------------------------------------------------------

#[test]
fn set_secret_nonexistent_env_fails() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "nope", "val"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report env not found, got: {stderr}"
    );
}

#[test]
fn get_secret_nonexistent_env_fails() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let stderr = zuul_err(bin, dir.path(), &["secret", "get", "KEY", "--env", "nope"]);
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report not found, got: {stderr}"
    );
}

#[test]
fn export_nonexistent_env_fails() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let stderr = zuul_err(
        bin,
        dir.path(),
        &["export", "--env", "nope", "--export-format", "dotenv"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("nope"),
        "should report env not found, got: {stderr}"
    );
}

#[test]
fn run_nonexistent_env_fails() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let stderr = zuul_err(
        bin,
        dir.path(),
        &["run", "--env", "nope", "--", "echo", "hi"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("nope"),
        "should report env not found, got: {stderr}"
    );
}

#[test]
fn import_nonexistent_env_fails() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let env_file = dir.path().join("secrets.env");
    std::fs::write(&env_file, "KEY=val\n").unwrap();

    let stderr = zuul_err(
        bin,
        dir.path(),
        &[
            "import",
            "--env",
            "nope",
            "--file",
            env_file.to_str().unwrap(),
        ],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("nope"),
        "should report env not found, got: {stderr}"
    );
}

#[test]
fn diff_nonexistent_env_fails() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let stderr = zuul_err(bin, dir.path(), &["diff", "dev", "nope"]);
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("nope"),
        "should report env not found, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Secret info on nonexistent secret
// ---------------------------------------------------------------------------

#[test]
fn info_nonexistent_secret_fails() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let stderr = zuul_err(bin, dir.path(), &["secret", "info", "NOPE", "--env", "dev"]);
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report not found, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Name validation at CLI level
// ---------------------------------------------------------------------------

#[test]
fn env_name_validation() {
    let dir = setup_project();
    let bin = zuul_bin();

    // Uppercase not allowed
    let stderr = zuul_err(bin, dir.path(), &["env", "create", "INVALID"]);
    assert!(
        !stderr.is_empty(),
        "should reject uppercase env name, got empty stderr"
    );

    // Double underscore not allowed
    let stderr = zuul_err(bin, dir.path(), &["env", "create", "has__double"]);
    assert!(
        stderr.contains("__"),
        "should reject __ in env name, got: {stderr}"
    );

    // Leading hyphen not allowed
    let stderr = zuul_err(bin, dir.path(), &["env", "create", "-leading"]);
    // Note: clap may interpret this as a flag, so the error might be from clap
    assert!(
        !stderr.is_empty(),
        "should reject leading hyphen, got empty stderr"
    );
}

#[test]
fn secret_name_validation() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    // Double underscore not allowed
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "set", "BAD__NAME", "--env", "dev", "val"],
    );
    assert!(
        stderr.contains("__"),
        "should reject __ in secret name, got: {stderr}"
    );

    // Starting with digit not allowed
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "set", "0DIGIT", "--env", "dev", "val"],
    );
    assert!(
        !stderr.is_empty(),
        "should reject digit-leading secret name, got empty stderr"
    );
}

// ---------------------------------------------------------------------------
// Deleting secret in one env doesn't affect another
// ---------------------------------------------------------------------------

#[test]
fn deleting_secret_in_one_env_does_not_affect_other() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "dev-val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "staging", "stg-val"],
    );

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "delete", "KEY", "-e", "dev", "--force"],
    );

    // Staging secret should still exist
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "KEY", "-e", "staging"]);
    assert_eq!(stdout.trim(), "stg-val");

    // Dev secret should be gone
    let stderr = zuul_err(bin, dir.path(), &["secret", "get", "KEY", "-e", "dev"]);
    assert!(stderr.contains("not") || stderr.contains("found"));
}

// ---------------------------------------------------------------------------
// Secret delete in wrong env fails
// ---------------------------------------------------------------------------

#[test]
fn secret_delete_wrong_env_fails() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "val"],
    );

    // Delete from staging where it doesn't exist
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "delete", "KEY", "-e", "staging", "--force"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report not found in staging, got: {stderr}"
    );

    // Dev secret still exists
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "KEY", "-e", "dev"]);
    assert_eq!(stdout.trim(), "val");
}

// ---------------------------------------------------------------------------
// JSON error output format
// ---------------------------------------------------------------------------

#[test]
fn json_format_on_error() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    // Error output should still be a plain error message (not JSON), even with --format json
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["--format", "json", "env", "show", "nonexistent"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report error, got: {stderr}"
    );
}
