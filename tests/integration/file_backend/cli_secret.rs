//! CLI-level secret tests for the file backend.

use crate::helpers::*;

// ---------------------------------------------------------------------------
// secret set + get
// ---------------------------------------------------------------------------

#[test]
fn secret_set_get_positional_value() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "set",
            "DB_URL",
            "--env",
            "dev",
            "postgres://localhost",
        ],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "DB_URL", "--env", "dev"],
    );
    assert_eq!(stdout.trim(), "postgres://localhost");
}

#[test]
fn secret_set_from_file() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let cert_file = dir.path().join("cert.pem");
    std::fs::write(&cert_file, "-----BEGIN CERT-----\ndata\n-----END CERT-----").unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "set",
            "TLS_CERT",
            "--env",
            "dev",
            "--from-file",
            cert_file.to_str().unwrap(),
        ],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "TLS_CERT", "--env", "dev"],
    );
    assert!(stdout.contains("BEGIN CERT"));
}

#[test]
fn secret_set_from_stdin() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let output = zuul_stdin(
        bin,
        dir.path(),
        &["secret", "set", "STDIN_KEY", "--env", "dev", "--from-stdin"],
        "stdin-value",
    );
    assert!(output.status.success());

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "STDIN_KEY", "--env", "dev"],
    );
    assert_eq!(stdout.trim(), "stdin-value");
}

#[test]
fn secret_set_overwrites_existing() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "dev", "old"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "dev", "new"],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "KEY", "--env", "dev"]);
    assert_eq!(stdout.trim(), "new");
}

// ---------------------------------------------------------------------------
// secret list
// ---------------------------------------------------------------------------

#[test]
fn secret_list_with_env() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "A", "--env", "dev", "a"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "B", "--env", "dev", "b"],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "list", "--env", "dev"]);
    assert!(stdout.contains("A"));
    assert!(stdout.contains("B"));
}

#[test]
fn secret_list_json_format() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "dev", "val"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["--format", "json", "secret", "list", "--env", "dev"],
    );
    let json = parse_json(&stdout);
    assert!(json.is_array());
}

#[test]
fn secret_list_cross_env() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "SHARED", "--env", "dev", "d"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "SHARED", "--env", "staging", "s"],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "list"]);
    assert!(stdout.contains("SHARED"));
    assert!(stdout.contains("dev"));
    assert!(stdout.contains("staging"));
}

#[test]
fn secret_list_cross_env_shows_correct_envs() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "SHARED_KEY", "--env", "dev", "dev_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "SHARED_KEY", "--env", "staging", "stg_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "DEV_ONLY", "--env", "dev", "x"],
    );

    // List without --env shows cross-env view
    let stdout = zuul_ok(bin, dir.path(), &["secret", "list"]);
    assert!(
        stdout.contains("SHARED_KEY"),
        "should contain SHARED_KEY, got: {stdout}"
    );
    assert!(
        stdout.contains("DEV_ONLY"),
        "should contain DEV_ONLY, got: {stdout}"
    );
}

#[test]
fn secret_list_with_metadata() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "API_KEY", "--env", "dev", "key123"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "metadata",
            "set",
            "API_KEY",
            "--env",
            "dev",
            "owner",
            "backend-team",
        ],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "list", "--env", "dev", "--with-metadata"],
    );
    assert!(
        stdout.contains("API_KEY"),
        "should contain secret name, got: {stdout}"
    );
    assert!(
        stdout.contains("owner") || stdout.contains("backend-team"),
        "should show metadata, got: {stdout}"
    );
}

#[test]
fn secret_list_with_metadata_json() {
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
            "secret", "metadata", "set", "KEY", "--env", "dev", "owner", "team-x",
        ],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &[
            "--format",
            "json",
            "secret",
            "list",
            "--env",
            "dev",
            "--with-metadata",
        ],
    );
    let json = parse_json(&stdout);
    assert!(json.is_array(), "should be JSON array, got: {stdout}");
}

// ---------------------------------------------------------------------------
// secret delete
// ---------------------------------------------------------------------------

#[test]
fn secret_delete_force() {
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
        &["secret", "delete", "KEY", "--env", "dev", "--force"],
    );

    let stderr = zuul_err(bin, dir.path(), &["secret", "get", "KEY", "--env", "dev"]);
    assert!(stderr.contains("not") || stderr.contains("found"));
}

#[test]
fn secret_delete_nonexistent_fails() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "delete", "NOPE", "--env", "dev", "--force"],
    );
    assert!(stderr.contains("not") || stderr.contains("found"));
}

// ---------------------------------------------------------------------------
// secret info
// ---------------------------------------------------------------------------

#[test]
fn secret_info_single_env() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "DB_URL", "--env", "dev", "postgres://"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "info", "DB_URL", "--env", "dev"],
    );
    assert!(stdout.contains("DB_URL"));
    assert!(stdout.contains("dev"));
}

#[test]
fn secret_info_shows_environments() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "DB_URL", "--env", "dev", "d"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "DB_URL", "--env", "staging", "s"],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "info", "DB_URL"]);
    assert!(stdout.contains("dev"));
    assert!(stdout.contains("staging"));
}

// ---------------------------------------------------------------------------
// secret copy
// ---------------------------------------------------------------------------

#[test]
fn secret_copy_basic() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "dev", "val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret", "copy", "KEY", "--from", "dev", "--to", "staging", "--force",
        ],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "KEY", "--env", "staging"],
    );
    assert_eq!(stdout.trim(), "val");
}

#[test]
fn secret_copy_existing_without_force_fails() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "dev", "dev_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "staging", "stg_val"],
    );

    // Copy without --force when target already has the secret -> should fail
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "copy", "KEY", "--from", "dev", "--to", "staging"],
    );
    assert!(
        stderr.contains("exists")
            || stderr.contains("already")
            || stderr.contains("Confirmation")
            || stderr.contains("force"),
        "should refuse without --force, got: {stderr}"
    );

    // Original staging value should be unchanged
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "KEY", "--env", "staging"],
    );
    assert_eq!(stdout.trim(), "stg_val");
}

#[test]
fn secret_copy_force_overwrites() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "URL", "--env", "dev", "new_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "URL", "--env", "staging", "old_val"],
    );

    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret", "copy", "URL", "--from", "dev", "--to", "staging", "--force",
        ],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "URL", "--env", "staging"],
    );
    assert_eq!(stdout.trim(), "new_val");
}

#[test]
fn secret_copy_to_nonexistent_env_fails() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "--env", "dev", "val"],
    );

    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "copy", "KEY", "--from", "dev", "--to", "ghost"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("exist"),
        "should report target env not found, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// secret delete --dry-run
// ---------------------------------------------------------------------------

#[test]
fn secret_delete_dry_run() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEEP", "--env", "dev", "val"],
    );

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "delete", "KEEP", "--env", "dev", "--dry-run"],
    );

    // Secret should still exist
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "KEEP", "--env", "dev"]);
    assert_eq!(stdout.trim(), "val");
}

// ---------------------------------------------------------------------------
// secret info --format json
// ---------------------------------------------------------------------------

#[test]
fn secret_info_json_format() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "API", "--env", "dev", "x"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["--format", "json", "secret", "info", "API", "--env", "dev"],
    );
    let json = parse_json(&stdout);
    assert!(json.is_object(), "info JSON should be an object");
}

// ---------------------------------------------------------------------------
// multiline and special character values
// ---------------------------------------------------------------------------

#[test]
fn secret_roundtrip_multiline_value() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let multiline = "line1\nline2\nline3";
    let file_path = dir.path().join("multi.txt");
    std::fs::write(&file_path, multiline).unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "set",
            "MULTI",
            "--env",
            "dev",
            "--from-file",
            file_path.to_str().unwrap(),
        ],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "MULTI", "--env", "dev"]);
    assert_eq!(stdout.trim(), multiline);
}

#[test]
fn secret_roundtrip_special_chars() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    // Value with quotes, equals, spaces, backslashes
    let special = r#"pa$$w0rd="hello world" key=val\ end"#;
    let file_path = dir.path().join("special.txt");
    std::fs::write(&file_path, special).unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "set",
            "SPECIAL",
            "--env",
            "dev",
            "--from-file",
            file_path.to_str().unwrap(),
        ],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "SPECIAL", "--env", "dev"],
    );
    assert_eq!(stdout.trim(), special);
}

// ---------------------------------------------------------------------------
// secret set with no value in non-interactive mode
// ---------------------------------------------------------------------------

#[test]
fn secret_set_no_value_non_interactive_fails() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    // No positional value, no --from-file, no --from-stdin -> interactive prompt -> fail
    let stderr = zuul_err(bin, dir.path(), &["secret", "set", "KEY", "--env", "dev"]);
    assert!(
        stderr.contains("Input required")
            || stderr.contains("non-interactive")
            || stderr.contains("--from-file")
            || stderr.contains("--from-stdin"),
        "should explain how to provide value, got: {stderr}"
    );
}
