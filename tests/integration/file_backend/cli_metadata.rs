//! CLI-level metadata tests for the file backend.

use crate::helpers::*;

// ---------------------------------------------------------------------------
// metadata set / list (single env)
// ---------------------------------------------------------------------------

#[test]
fn metadata_set_and_list_single_env() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "DB_URL", "--env", "dev", "postgres://"],
    );

    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "metadata",
            "set",
            "DB_URL",
            "--env",
            "dev",
            "owner",
            "backend-team",
        ],
    );
    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "metadata",
            "set",
            "DB_URL",
            "--env",
            "dev",
            "rotate-by",
            "2026-06-01",
        ],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "list", "DB_URL", "--env", "dev"],
    );
    assert!(
        stdout.contains("owner"),
        "should list owner key, got: {stdout}"
    );
    assert!(
        stdout.contains("backend-team"),
        "should list owner value, got: {stdout}"
    );
    assert!(
        stdout.contains("rotate-by"),
        "should list rotate-by key, got: {stdout}"
    );
    assert!(
        stdout.contains("2026-06-01"),
        "should list rotate-by value, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// metadata delete
// ---------------------------------------------------------------------------

#[test]
fn metadata_delete_single_env() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "dev", "val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret", "metadata", "set", "KEY", "--env", "dev", "owner", "me",
        ],
    );

    // Delete should succeed
    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret", "metadata", "delete", "KEY", "--env", "dev", "owner",
        ],
    );
}

// ---------------------------------------------------------------------------
// metadata cross-env (no --env flag)
// ---------------------------------------------------------------------------

#[test]
fn metadata_set_cross_env() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "SHARED", "--env", "dev", "v1"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "SHARED", "--env", "staging", "v2"],
    );

    // Set metadata without --env -> should apply to all envs
    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "metadata",
            "set",
            "SHARED",
            "owner",
            "platform-team",
        ],
    );

    // Verify in dev
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "list", "SHARED", "--env", "dev"],
    );
    assert!(
        stdout.contains("platform-team"),
        "dev should have metadata, got: {stdout}"
    );

    // Verify in staging
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "list", "SHARED", "--env", "staging"],
    );
    assert!(
        stdout.contains("platform-team"),
        "staging should have metadata, got: {stdout}"
    );
}

#[test]
fn metadata_delete_cross_env() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "dev", "a"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "staging", "b"],
    );

    // Set in both envs
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "set", "KEY", "tag", "v1"],
    );

    // Delete without --env -> should succeed (removes from all)
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "delete", "KEY", "tag"],
    );
}

#[test]
fn metadata_list_cross_env() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "dev", "a"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "staging", "b"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "set", "KEY", "owner", "ops"],
    );

    // List without --env -> should show metadata across envs
    let stdout = zuul_ok(bin, dir.path(), &["secret", "metadata", "list", "KEY"]);
    assert!(
        stdout.contains("owner"),
        "cross-env list should show metadata, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// metadata on nonexistent secret
// ---------------------------------------------------------------------------

#[test]
fn metadata_set_nonexistent_secret_fails() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let stderr = zuul_err(
        bin,
        dir.path(),
        &[
            "secret", "metadata", "set", "GHOST", "--env", "dev", "owner", "me",
        ],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report secret not found, got: {stderr}"
    );
}
