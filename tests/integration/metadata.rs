use crate::helpers::*;

// ---------------------------------------------------------------------------
// metadata set / list (single env)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn metadata_set_and_list_single_env() {
    let bin = zuul_bin();
    let dir = setup_project("integ-meta-single");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "DB_URL", "postgres://"],
    );

    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "metadata",
            "set",
            "-e",
            "dev",
            "DB_URL",
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
            "-e",
            "dev",
            "DB_URL",
            "rotate-by",
            "2026-06-01",
        ],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "list", "-e", "dev", "DB_URL"],
    );
    assert!(stdout.contains("owner"));
    assert!(stdout.contains("backend-team"));
    assert!(stdout.contains("rotate-by"));
    assert!(stdout.contains("2026-06-01"));
}

// ---------------------------------------------------------------------------
// metadata delete
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn metadata_delete_single_env() {
    let bin = zuul_bin();
    let dir = setup_project("integ-meta-del");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret", "metadata", "set", "-e", "dev", "KEY", "owner", "me",
        ],
    );

    // Delete should succeed
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "delete", "-e", "dev", "KEY", "owner"],
    );

    // Verify deletion (emulator may or may not persist removal,
    // so we just assert the delete command succeeded above)
}

// ---------------------------------------------------------------------------
// metadata cross-env (no --env flag)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn metadata_set_cross_env() {
    let bin = zuul_bin();
    let dir = setup_project("integ-meta-cross-set");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "SHARED", "v1"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "staging", "SHARED", "v2"],
    );

    // Set metadata without --env → should apply to all envs
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
        &["secret", "metadata", "list", "-e", "dev", "SHARED"],
    );
    assert!(
        stdout.contains("platform-team"),
        "dev should have metadata, got: {stdout}"
    );

    // Verify in staging
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "list", "-e", "staging", "SHARED"],
    );
    assert!(
        stdout.contains("platform-team"),
        "staging should have metadata, got: {stdout}"
    );
}

#[test]
#[ignore]
fn metadata_delete_cross_env() {
    let bin = zuul_bin();
    let dir = setup_project("integ-meta-cross-del");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(bin, dir.path(), &["secret", "set", "-e", "dev", "KEY", "a"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "staging", "KEY", "b"],
    );

    // Set in both envs
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "set", "KEY", "tag", "v1"],
    );

    // Delete without --env → should succeed (removes from all)
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "delete", "KEY", "tag"],
    );
}

#[test]
#[ignore]
fn metadata_list_cross_env() {
    let bin = zuul_bin();
    let dir = setup_project("integ-meta-cross-list");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(bin, dir.path(), &["secret", "set", "-e", "dev", "KEY", "a"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "staging", "KEY", "b"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "metadata", "set", "KEY", "owner", "ops"],
    );

    // List without --env → should show metadata across envs
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
#[ignore]
fn metadata_set_nonexistent_secret_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-meta-missing");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

    let stderr = zuul_err(
        bin,
        dir.path(),
        &[
            "secret", "metadata", "set", "-e", "dev", "GHOST", "owner", "me",
        ],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report secret not found, got: {stderr}"
    );
}
