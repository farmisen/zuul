//! CLI-level audit tests for the file backend.

use crate::helpers::*;

// ---------------------------------------------------------------------------
// audit returns unsupported error for file backend
// ---------------------------------------------------------------------------

#[test]
fn audit_unsupported_for_file_backend() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

    let stderr = zuul_err(bin, dir.path(), &["audit"]);
    assert!(
        stderr.contains("not available") || stderr.contains("Unsupported"),
        "should report audit is unsupported for file backend, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// audit with --format json also returns unsupported
// ---------------------------------------------------------------------------

#[test]
fn audit_json_unsupported_for_file_backend() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

    let stderr = zuul_err(bin, dir.path(), &["audit", "--format", "json"]);
    assert!(
        stderr.contains("not available") || stderr.contains("Unsupported"),
        "should report audit is unsupported for file backend, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// audit with --env filter still returns unsupported
// ---------------------------------------------------------------------------

#[test]
fn audit_with_env_filter_unsupported() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

    let stderr = zuul_err(bin, dir.path(), &["audit", "--env", "dev"]);
    assert!(
        stderr.contains("not available") || stderr.contains("Unsupported"),
        "should report audit is unsupported for file backend, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// audit fails without config
// ---------------------------------------------------------------------------

#[test]
fn audit_fails_without_config() {
    let dir = tempfile::tempdir().unwrap();
    let bin = zuul_bin();

    let stderr = zuul_err(bin, dir.path(), &["audit"]);
    assert!(
        stderr.contains("No .zuul.toml found") || stderr.contains("zuul init"),
        "should mention missing config, got: {stderr}"
    );
}
