//! CLI-level environment tests for the file backend.

use crate::helpers::*;

// ---------------------------------------------------------------------------
// env create
// ---------------------------------------------------------------------------

#[test]
fn env_create_basic() {
    let dir = setup_project();
    let bin = zuul_bin();

    let stdout = zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    assert!(
        stdout.contains("dev"),
        "should confirm creation, got: {stdout}"
    );
}

#[test]
fn env_create_with_description() {
    let dir = setup_project();
    let bin = zuul_bin();

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
fn env_create_duplicate_fails() {
    let dir = setup_project();
    let bin = zuul_bin();

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
fn env_list_text_and_json() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);

    let stdout = zuul_ok(bin, dir.path(), &["env", "list"]);
    assert!(stdout.contains("dev"));
    assert!(stdout.contains("staging"));

    let stdout = zuul_ok(bin, dir.path(), &["--format", "json", "env", "list"]);
    let json = parse_json(&stdout);
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 2);
}

#[test]
fn env_list_empty() {
    let dir = setup_project();
    let bin = zuul_bin();

    let stdout = zuul_ok(bin, dir.path(), &["env", "list"]);
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
fn env_show_text_and_json() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["env", "create", "dev", "--description", "Development"],
    );

    let stdout = zuul_ok(bin, dir.path(), &["env", "show", "dev"]);
    assert!(stdout.contains("dev"));
    assert!(stdout.contains("Development"));

    let stdout = zuul_ok(bin, dir.path(), &["--format", "json", "env", "show", "dev"]);
    let json = parse_json(&stdout);
    assert_eq!(json["name"].as_str().unwrap(), "dev");
}

#[test]
fn env_show_nonexistent_fails() {
    let dir = setup_project();
    let bin = zuul_bin();

    let stderr = zuul_err(bin, dir.path(), &["env", "show", "nope"]);
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report not found, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// env update
// ---------------------------------------------------------------------------

#[test]
fn env_update_description() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["env", "update", "dev", "--description", "Updated"],
    );
    let stdout = zuul_ok(bin, dir.path(), &["env", "show", "dev"]);
    assert!(stdout.contains("Updated"), "got: {stdout}");
}

// ---------------------------------------------------------------------------
// env delete
// ---------------------------------------------------------------------------

#[test]
fn env_delete_with_force() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "ephemeral"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "ephemeral", "KEY", "val"],
    );
    zuul_ok(bin, dir.path(), &["env", "delete", "ephemeral", "--force"]);

    let stdout = zuul_ok(bin, dir.path(), &["env", "list"]);
    assert!(
        !stdout.contains("ephemeral"),
        "env should be deleted, got: {stdout}"
    );
}

#[test]
fn env_delete_nonexistent_fails() {
    let dir = setup_project();
    let bin = zuul_bin();

    let stderr = zuul_err(bin, dir.path(), &["env", "delete", "ghost", "--force"]);
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report not found, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// env copy
// ---------------------------------------------------------------------------

#[test]
fn env_copy_basic() {
    let dir = setup_project();
    let bin = zuul_bin();

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
        &["env", "copy", "source", "target", "--force"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "-e", "target", "DB_URL"],
    );
    assert_eq!(stdout.trim(), "postgres://src");
}

// ---------------------------------------------------------------------------
// env clear
// ---------------------------------------------------------------------------

#[test]
fn env_clear_removes_secrets_keeps_env() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "val"],
    );
    zuul_ok(bin, dir.path(), &["env", "clear", "dev", "--force"]);

    // Environment still exists
    let stdout = zuul_ok(bin, dir.path(), &["env", "show", "dev"]);
    assert!(stdout.contains("dev"));

    // Secret gone
    let stderr = zuul_err(bin, dir.path(), &["secret", "get", "-e", "dev", "KEY"]);
    assert!(stderr.contains("not") || stderr.contains("found"));
}

#[test]
fn env_clear_dry_run() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "val"],
    );

    zuul_ok(bin, dir.path(), &["env", "clear", "dev", "--dry-run"]);

    // Secret should still exist after dry run
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "KEY"]);
    assert_eq!(stdout.trim(), "val");
}

// ---------------------------------------------------------------------------
// env copy --dry-run
// ---------------------------------------------------------------------------

#[test]
fn env_copy_dry_run() {
    let dir = setup_project();
    let bin = zuul_bin();

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
// env copy with overlapping secrets in target (--force overwrites)
// ---------------------------------------------------------------------------

#[test]
fn env_copy_with_overlapping_secrets() {
    let dir = setup_project();
    let bin = zuul_bin();

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
