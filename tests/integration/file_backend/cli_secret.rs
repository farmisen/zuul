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
