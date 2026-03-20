use crate::helpers::*;

// ---------------------------------------------------------------------------
// env list
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn env_list_text_and_json() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-list");

    create_envs(&dir, &["dev", "staging"]);

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
#[ignore = "needs emulator"]
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
#[ignore = "needs emulator"]
fn env_show_text_and_json() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-show");

    create_envs(&dir, &["dev"]);

    // Text
    let stdout = zuul_ok(bin, dir.path(), &["env", "show", "dev"]);
    assert!(stdout.contains("dev"));

    // JSON
    let stdout = zuul_ok(bin, dir.path(), &["--format", "json", "env", "show", "dev"]);
    let json = parse_json(&stdout);
    assert_eq!(json["name"].as_str().unwrap(), "dev");
}

#[test]
#[ignore = "needs emulator"]
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
// env copy
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn env_copy_basic() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-copy");

    create_envs(&dir, &["source", "target"]);
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
#[ignore = "needs emulator"]
fn env_copy_dry_run() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-copy-dry");

    create_envs(&dir, &["src", "dst"]);
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
#[ignore = "needs emulator"]
fn env_clear_removes_secrets_keeps_env() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-clear");

    create_envs(&dir, &["dev"]);
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
#[ignore = "needs emulator"]
fn env_clear_dry_run() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-clear-dry");

    create_envs(&dir, &["dev"]);
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
// env copy with overlapping secrets in target (--force overwrites)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn env_copy_with_overlapping_secrets() {
    let bin = zuul_bin();
    let dir = setup_project("integ-env-copy-overlap");

    create_envs(&dir, &["source", "target"]);

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
