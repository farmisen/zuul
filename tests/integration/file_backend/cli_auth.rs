//! CLI-level auth tests for the file backend.

use crate::helpers::*;

// ---------------------------------------------------------------------------
// auth --check succeeds when store is decryptable
// ---------------------------------------------------------------------------

#[test]
fn auth_check_succeeds_with_valid_identity() {
    let dir = setup_project();
    let bin = zuul_bin();

    // Create an environment so the store exists and is non-empty
    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

    let output = zuul(bin, dir.path(), &["auth", "--check"]);
    assert!(
        output.status.success(),
        "auth --check should succeed with valid identity, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------------------------
// auth --check fails when no config file
// ---------------------------------------------------------------------------

#[test]
fn auth_check_fails_without_config() {
    let dir = tempfile::tempdir().unwrap();
    let bin = zuul_bin();

    let stderr = zuul_err(bin, dir.path(), &["auth", "--check"]);
    assert!(
        stderr.contains("No .zuul.toml found") || stderr.contains("zuul init"),
        "should mention missing config, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// auth (no --check) prints success info
// ---------------------------------------------------------------------------

#[test]
fn auth_prints_success_info() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

    let stdout = zuul_ok(bin, dir.path(), &["auth"]);
    assert!(
        stdout.contains("Authentication valid") || stdout.contains("✔"),
        "should report authentication valid, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// auth shows environments when they exist
// ---------------------------------------------------------------------------

#[test]
fn auth_shows_available_environments() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);

    let stdout = zuul_ok(bin, dir.path(), &["auth"]);
    assert!(
        stdout.contains("dev") && stdout.contains("staging"),
        "should list available environments, got: {stdout}"
    );
}
