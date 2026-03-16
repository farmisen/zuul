use crate::helpers::*;

// ---------------------------------------------------------------------------
// env create
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_create_basic() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-create");

    let stdout = zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    assert!(
        stdout.contains("dev"),
        "should confirm creation, got: {stdout}"
    );
}

#[test]
#[ignore]
fn env_create_with_description() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-create-desc");

    zuul_ok(
        bin,
        dir.path(),
        &["env", "create", "staging", "--description", "Staging env"],
    );
    let stdout = zuul_ok(bin, dir.path(), &["env", "show", "staging"]);
    assert!(
        stdout.contains("Staging env"),
        "should show description, got: {stdout}"
    );
}

#[test]
#[ignore]
fn env_create_duplicate_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-create-dup");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    let stderr = zuul_err(bin, dir.path(), &["env", "create", "dev"]);
    assert!(
        stderr.contains("already exists") || stderr.contains("AlreadyExists"),
        "should report duplicate, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// env list
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_list_text_and_json() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-list");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);

    // Text output
    let stdout = zuul_ok(bin, dir.path(), &["env", "list"]);
    assert!(stdout.contains("dev"));
    assert!(stdout.contains("staging"));

    // JSON output
    let stdout = zuul_ok(bin, dir.path(), &["--format", "json", "env", "list"]);
    let json = parse_json(&stdout);
    assert!(json.is_array());
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);
}

#[test]
#[ignore]
fn env_list_empty() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-list-empty");

    let stdout = zuul_ok(bin, dir.path(), &["env", "list"]);
    // Should succeed with no environments (empty list or message)
    assert!(
        stdout.is_empty()
            || stdout.contains("No environments")
            || stdout.trim().lines().count() <= 2,
        "should handle empty list gracefully, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// env show
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_show_text_and_json() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-show");

    zuul_ok(
        bin,
        dir.path(),
        &["env", "create", "dev", "--description", "Development"],
    );

    // Text
    let stdout = zuul_ok(bin, dir.path(), &["env", "show", "dev"]);
    assert!(stdout.contains("dev"));
    assert!(stdout.contains("Development"));

    // JSON
    let stdout = zuul_ok(bin, dir.path(), &["--format", "json", "env", "show", "dev"]);
    let json = parse_json(&stdout);
    assert_eq!(json["name"].as_str().unwrap(), "dev");
}

#[test]
#[ignore]
fn env_show_nonexistent_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-show-missing");

    let stderr = zuul_err(bin, dir.path(), &["env", "show", "nope"]);
    assert!(
        stderr.contains("not") || stderr.contains("exist") || stderr.contains("found"),
        "should report not found, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// env update
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_update_description() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-update-desc");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["env", "update", "dev", "--description", "Updated desc"],
    );
    let stdout = zuul_ok(bin, dir.path(), &["env", "show", "dev"]);
    assert!(
        stdout.contains("Updated desc"),
        "description should be updated, got: {stdout}"
    );
}

#[test]
#[ignore]
fn env_update_rename_with_force() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-rename-force");

    zuul_ok(bin, dir.path(), &["env", "create", "old-name"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "old-name", "MY_KEY", "my_val"],
    );

    // Rename with --force bypasses confirmation
    zuul_ok(
        bin,
        dir.path(),
        &[
            "env",
            "update",
            "old-name",
            "--new-name",
            "new-name",
            "--force",
        ],
    );

    // Old name should no longer exist in registry
    let stderr = zuul_err(bin, dir.path(), &["env", "show", "old-name"]);
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "old env should be gone, got: {stderr}"
    );

    // New name should exist in registry
    let stdout = zuul_ok(bin, dir.path(), &["env", "show", "new-name"]);
    assert!(stdout.contains("new-name"));

    // Note: The underlying GCP secrets are NOT yet renamed by update_environment.
    // The backend only updates the registry. Secret renaming is a known gap
    // (update_environment does not rename zuul__old-name__* to zuul__new-name__*).
}

// ---------------------------------------------------------------------------
// env delete
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_delete_dry_run() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-del-dry");

    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);

    let stdout = zuul_ok(bin, dir.path(), &["env", "delete", "staging", "--dry-run"]);
    assert!(
        stdout.contains("dry") || stdout.contains("Dry") || stdout.contains("would"),
        "should indicate dry run, got: {stdout}"
    );

    // Environment should still exist
    let stdout = zuul_ok(bin, dir.path(), &["env", "list"]);
    assert!(
        stdout.contains("staging"),
        "staging should survive dry run, got: {stdout}"
    );
}

#[test]
#[ignore]
fn env_delete_force_cascades_secrets() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-del-cascade");

    zuul_ok(bin, dir.path(), &["env", "create", "ephemeral"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "ephemeral", "KEY_A", "val_a"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "ephemeral", "KEY_B", "val_b"],
    );

    // Delete with --force bypasses both confirmation prompts
    zuul_ok(bin, dir.path(), &["env", "delete", "ephemeral", "--force"]);

    // Environment should be gone
    let stdout = zuul_ok(bin, dir.path(), &["env", "list"]);
    assert!(
        !stdout.contains("ephemeral"),
        "env should be deleted, got: {stdout}"
    );
}

#[test]
#[ignore]
fn env_delete_nonexistent_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-del-missing");

    let stderr = zuul_err(bin, dir.path(), &["env", "delete", "ghost"]);
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("exist"),
        "should report not found, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// env copy
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_copy_basic() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-copy");

    zuul_ok(bin, dir.path(), &["env", "create", "source"]);
    zuul_ok(bin, dir.path(), &["env", "create", "target"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "source", "DB_URL", "postgres://src"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "source", "API_KEY", "key123"],
    );

    zuul_ok(
        bin,
        dir.path(),
        &["env", "copy", "source", "target", "--force"],
    );

    // Verify secrets were copied
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "-e", "target", "DB_URL"],
    );
    assert_eq!(stdout.trim(), "postgres://src");

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "-e", "target", "API_KEY"],
    );
    assert_eq!(stdout.trim(), "key123");
}

#[test]
#[ignore]
fn env_copy_dry_run() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-copy-dry");

    zuul_ok(bin, dir.path(), &["env", "create", "src"]);
    zuul_ok(bin, dir.path(), &["env", "create", "dst"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "src", "KEY", "val"],
    );

    zuul_ok(bin, dir.path(), &["env", "copy", "src", "dst", "--dry-run"]);

    // Secret should NOT have been copied
    let stderr = zuul_err(bin, dir.path(), &["secret", "get", "-e", "dst", "KEY"]);
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "secret should not exist after dry run, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// env clear
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_clear_removes_secrets_keeps_env() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-clear");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY_A", "a"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY_B", "b"],
    );

    zuul_ok(bin, dir.path(), &["env", "clear", "dev", "--force"]);

    // Environment should still exist
    let stdout = zuul_ok(bin, dir.path(), &["env", "show", "dev"]);
    assert!(stdout.contains("dev"));

    // Secrets should be gone
    let stdout = zuul_ok(bin, dir.path(), &["secret", "list", "-e", "dev"]);
    assert!(
        !stdout.contains("KEY_A") && !stdout.contains("KEY_B"),
        "secrets should be cleared, got: {stdout}"
    );
}

#[test]
#[ignore]
fn env_clear_dry_run() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-clear-dry");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "val"],
    );

    zuul_ok(bin, dir.path(), &["env", "clear", "dev", "--dry-run"]);

    // Secret should still exist
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "KEY"]);
    assert_eq!(stdout.trim(), "val");
}

// ---------------------------------------------------------------------------
// env delete without --force when secrets exist (should refuse)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_delete_without_force_refuses_when_secrets_exist() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-del-noforce");

    zuul_ok(bin, dir.path(), &["env", "create", "target"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "target", "KEY", "val"],
    );

    // Without --force in non-interactive mode → should refuse
    let stderr = zuul_err(bin, dir.path(), &["env", "delete", "target"]);
    assert!(
        stderr.contains("Confirmation") || stderr.contains("force") || stderr.contains("confirm"),
        "should refuse without --force, got: {stderr}"
    );

    // Environment should still exist
    let stdout = zuul_ok(bin, dir.path(), &["env", "show", "target"]);
    assert!(stdout.contains("target"));
}

// ---------------------------------------------------------------------------
// env copy with overlapping secrets in target (--force overwrites)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_copy_with_overlapping_secrets() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-copy-overlap");

    zuul_ok(bin, dir.path(), &["env", "create", "source"]);
    zuul_ok(bin, dir.path(), &["env", "create", "target"]);

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "source", "SHARED", "src_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "source", "SRC_ONLY", "src_only"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "target", "SHARED", "tgt_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "target", "TGT_ONLY", "tgt_only"],
    );

    // Copy with --force should overwrite SHARED in target
    zuul_ok(
        bin,
        dir.path(),
        &["env", "copy", "source", "target", "--force"],
    );

    // SHARED should have source value
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "-e", "target", "SHARED"],
    );
    assert_eq!(stdout.trim(), "src_val");

    // SRC_ONLY should now exist in target
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "-e", "target", "SRC_ONLY"],
    );
    assert_eq!(stdout.trim(), "src_only");

    // TGT_ONLY should still exist (env copy doesn't remove target-only secrets)
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "-e", "target", "TGT_ONLY"],
    );
    assert_eq!(stdout.trim(), "tgt_only");
}
