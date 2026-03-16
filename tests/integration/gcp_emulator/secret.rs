use crate::helpers::*;

// ---------------------------------------------------------------------------
// secret set / get (various value sources)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn secret_set_get_positional_value() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-pos");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "MY_KEY", "hello"],
    );
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "MY_KEY"]);
    assert_eq!(stdout.trim(), "hello");
}

#[test]
#[ignore]
fn secret_set_from_file() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-file");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

    let cert_path = dir.path().join("cert.pem");
    std::fs::write(
        &cert_path,
        "-----BEGIN CERT-----\nABC123\n-----END CERT-----\n",
    )
    .unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "set",
            "-e",
            "dev",
            "TLS_CERT",
            "--from-file",
            cert_path.to_str().unwrap(),
        ],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "TLS_CERT"]);
    assert!(stdout.contains("-----BEGIN CERT-----"));
    assert!(stdout.contains("ABC123"));
}

#[test]
#[ignore]
fn secret_set_from_stdin() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-stdin");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

    let output = zuul_stdin(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "STDIN_KEY", "--from-stdin"],
        "piped_value",
    );
    assert!(
        output.status.success(),
        "set from stdin failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "-e", "dev", "STDIN_KEY"],
    );
    assert_eq!(stdout.trim(), "piped_value");
}

#[test]
#[ignore]
fn secret_set_overwrites_existing() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-overwrite");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "v1"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "v2"],
    );
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "KEY"]);
    assert_eq!(stdout.trim(), "v2");
}

// ---------------------------------------------------------------------------
// secret list
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn secret_list_with_env_accepts_flag() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-list-env");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "dev", "DEV_KEY", "x"],
    );

    // Note: The emulator does not support GCP label filtering, so `--env`
    // does not actually filter the result set. We verify the flag is accepted
    // and the output includes the secret we created.
    let stdout = zuul_ok(bin, dir.path(), &["secret", "list", "--env", "dev"]);
    assert!(
        stdout.contains("DEV_KEY"),
        "list should contain secret, got: {stdout}"
    );
}

#[test]
#[ignore]
fn secret_list_cross_env() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-list-cross");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "SHARED_KEY", "dev_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "staging", "SHARED_KEY", "stg_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "DEV_ONLY", "x"],
    );

    // List without --env shows cross-env view
    let stdout = zuul_ok(bin, dir.path(), &["secret", "list"]);
    assert!(stdout.contains("SHARED_KEY"));
    assert!(stdout.contains("DEV_ONLY"));
}

#[test]
#[ignore]
fn secret_list_with_metadata() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-list-meta");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "API_KEY", "key123"],
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
            "API_KEY",
            "owner",
            "backend-team",
        ],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "list", "-e", "dev", "--with-metadata"],
    );
    assert!(stdout.contains("API_KEY"));
    assert!(
        stdout.contains("owner") || stdout.contains("backend-team"),
        "should show metadata, got: {stdout}"
    );
}

#[test]
#[ignore]
fn secret_list_json_format() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-list-json");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY_A", "a"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["--format", "json", "secret", "list", "-e", "dev"],
    );
    let json = parse_json(&stdout);
    assert!(json.is_array(), "JSON list output should be array");
}

// ---------------------------------------------------------------------------
// secret delete
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn secret_delete_force() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-del-force");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "GONE", "val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "delete", "-e", "dev", "GONE", "--force"],
    );

    let stderr = zuul_err(bin, dir.path(), &["secret", "get", "-e", "dev", "GONE"]);
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "deleted secret should not be accessible, got: {stderr}"
    );
}

#[test]
#[ignore]
fn secret_delete_dry_run() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-del-dry");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEEP", "val"],
    );

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "delete", "-e", "dev", "KEEP", "--dry-run"],
    );

    // Secret should still exist
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "KEEP"]);
    assert_eq!(stdout.trim(), "val");
}

#[test]
#[ignore]
fn secret_delete_nonexistent_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-del-missing");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "delete", "-e", "dev", "GHOST", "--force"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report not found, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// secret info
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn secret_info_single_env() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-info-env");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "DB_URL", "postgres://"],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "info", "-e", "dev", "DB_URL"]);
    assert!(stdout.contains("DB_URL"));
    assert!(stdout.contains("dev"));
}

#[test]
#[ignore]
fn secret_info_cross_env() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-info-cross");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "SHARED", "a"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "staging", "SHARED", "b"],
    );

    // info without --env should show both environments
    let stdout = zuul_ok(bin, dir.path(), &["secret", "info", "SHARED"]);
    assert!(
        stdout.contains("dev") && stdout.contains("staging"),
        "should list both envs, got: {stdout}"
    );
}

#[test]
#[ignore]
fn secret_info_json_format() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-info-json");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["secret", "set", "-e", "dev", "API", "x"]);

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["--format", "json", "secret", "info", "-e", "dev", "API"],
    );
    let json = parse_json(&stdout);
    assert!(json.is_object(), "info JSON should be an object");
}

// ---------------------------------------------------------------------------
// secret copy
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn secret_copy_basic() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-copy");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "URL", "http://dev"],
    );

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "copy", "URL", "--from", "dev", "--to", "staging"],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "staging", "URL"]);
    assert_eq!(stdout.trim(), "http://dev");
}

#[test]
#[ignore]
fn secret_copy_force_overwrites() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-copy-force");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "URL", "new_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "staging", "URL", "old_val"],
    );

    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret", "copy", "URL", "--from", "dev", "--to", "staging", "--force",
        ],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "staging", "URL"]);
    assert_eq!(stdout.trim(), "new_val");
}

// ---------------------------------------------------------------------------
// secret copy without --force when target exists (should fail non-interactive)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn secret_copy_existing_without_force_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-copy-noforce");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "dev_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "staging", "KEY", "stg_val"],
    );

    // Copy without --force when target already has the secret → should fail
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
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "staging", "KEY"]);
    assert_eq!(stdout.trim(), "stg_val");
}

// ---------------------------------------------------------------------------
// secret copy to nonexistent target environment
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn secret_copy_to_nonexistent_env_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-copy-noenv");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "val"],
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
// secret set with no value in non-interactive mode
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn secret_set_no_value_non_interactive_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-set-noval");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

    // No positional value, no --from-file, no --from-stdin → interactive prompt → fail
    let stderr = zuul_err(bin, dir.path(), &["secret", "set", "-e", "dev", "KEY"]);
    assert!(
        stderr.contains("Input required")
            || stderr.contains("non-interactive")
            || stderr.contains("--from-file")
            || stderr.contains("--from-stdin"),
        "should explain how to provide value, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// secret list --with-metadata --format json (combined)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn secret_list_with_metadata_json() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-list-meta-json");

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
            "secret", "metadata", "set", "-e", "dev", "KEY", "owner", "team-x",
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
            "-e",
            "dev",
            "--with-metadata",
        ],
    );
    let json = parse_json(&stdout);
    assert!(json.is_array(), "should be JSON array, got: {stdout}");
}

// ---------------------------------------------------------------------------
// multiline and special character values
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn secret_roundtrip_multiline_value() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-multiline");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

    let multiline = "line1\nline2\nline3";
    let file_path = dir.path().join("multi.txt");
    std::fs::write(&file_path, multiline).unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "set",
            "-e",
            "dev",
            "MULTI",
            "--from-file",
            file_path.to_str().unwrap(),
        ],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "MULTI"]);
    assert_eq!(stdout.trim(), multiline);
}

#[test]
#[ignore]
fn secret_roundtrip_special_chars() {
    let bin = zuul_bin();
    let dir = setup_project("integ-secret-special");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

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
            "-e",
            "dev",
            "SPECIAL",
            "--from-file",
            file_path.to_str().unwrap(),
        ],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "SPECIAL"]);
    assert_eq!(stdout.trim(), special);
}
