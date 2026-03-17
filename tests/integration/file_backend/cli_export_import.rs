//! CLI-level export/import/run tests for the file backend.

use crate::helpers::*;

// ---------------------------------------------------------------------------
// export
// ---------------------------------------------------------------------------

#[test]
fn export_dotenv() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "secret",
            "set",
            "DB_URL",
            "-e",
            "dev",
            "postgres://localhost",
        ],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "API_KEY", "-e", "dev", "sk_test"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "--env", "dev", "--export-format", "dotenv"],
    );
    assert!(stdout.contains("DB_URL="));
    assert!(stdout.contains("API_KEY="));
}

#[test]
fn export_json() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "val"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "--env", "dev", "--export-format", "json"],
    );
    let json = parse_json(&stdout);
    assert_eq!(json["KEY"].as_str().unwrap(), "val");
}

#[test]
fn export_direnv() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "val"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "--env", "dev", "--export-format", "direnv"],
    );
    assert!(stdout.contains("export KEY="));
}

#[test]
fn export_yaml() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "val"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "--env", "dev", "--export-format", "yaml"],
    );
    assert!(stdout.contains("KEY:"));
}

#[test]
fn export_to_file() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "val"],
    );

    let out_file = dir.path().join("secrets.env");
    zuul_ok(
        bin,
        dir.path(),
        &[
            "export",
            "--env",
            "dev",
            "--export-format",
            "dotenv",
            "--output",
            out_file.to_str().unwrap(),
        ],
    );

    let content = std::fs::read_to_string(&out_file).unwrap();
    assert!(content.contains("KEY="));
}

// ---------------------------------------------------------------------------
// import
// ---------------------------------------------------------------------------

#[test]
fn import_dotenv() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let env_file = dir.path().join("secrets.env");
    std::fs::write(
        &env_file,
        "DB_URL=postgres://imported\nAPI_KEY=sk_imported\n",
    )
    .unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "import",
            "--env",
            "dev",
            "--file",
            env_file.to_str().unwrap(),
        ],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "DB_URL", "-e", "dev"]);
    assert_eq!(stdout.trim(), "postgres://imported");
}

#[test]
fn import_json() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let json_file = dir.path().join("secrets.json");
    std::fs::write(&json_file, r#"{"KEY": "json-val"}"#).unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "import",
            "--env",
            "dev",
            "--file",
            json_file.to_str().unwrap(),
        ],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "KEY", "-e", "dev"]);
    assert_eq!(stdout.trim(), "json-val");
}

#[test]
fn import_dry_run() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let env_file = dir.path().join("secrets.env");
    std::fs::write(&env_file, "KEY=value\n").unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "import",
            "--env",
            "dev",
            "--file",
            env_file.to_str().unwrap(),
            "--dry-run",
        ],
    );

    let stderr = zuul_err(bin, dir.path(), &["secret", "get", "KEY", "-e", "dev"]);
    assert!(stderr.contains("not") || stderr.contains("found"));
}

#[test]
fn import_overwrite_replaces_existing() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "old"],
    );

    let env_file = dir.path().join("secrets.env");
    std::fs::write(&env_file, "KEY=new\n").unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "import",
            "--env",
            "dev",
            "--file",
            env_file.to_str().unwrap(),
            "--overwrite",
        ],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "KEY", "-e", "dev"]);
    assert_eq!(stdout.trim(), "new");
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

#[test]
fn run_injects_secrets() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "TEST_VAR", "-e", "dev", "hello"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["run", "--env", "dev", "--", "sh", "-c", "echo $TEST_VAR"],
    );
    assert_eq!(stdout.trim(), "hello");
}

#[test]
fn run_forwards_exit_code() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let output = zuul(
        bin,
        dir.path(),
        &["run", "--env", "dev", "--", "sh", "-c", "exit 42"],
    );
    assert_eq!(output.status.code(), Some(42));
}

// ---------------------------------------------------------------------------
// diff
// ---------------------------------------------------------------------------

#[test]
fn diff_shows_differences() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "dev-val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "staging", "stg-val"],
    );

    let stdout = zuul_ok(bin, dir.path(), &["diff", "dev", "staging"]);
    assert!(
        stdout.contains("KEY"),
        "should list the secret name, got: {stdout}"
    );
}

#[test]
fn diff_empty_envs() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "a"]);
    zuul_ok(bin, dir.path(), &["env", "create", "b"]);

    let stdout = zuul_ok(bin, dir.path(), &["diff", "a", "b"]);
    assert!(
        stdout.contains("No differences")
            || stdout.contains("identical")
            || stdout.trim().is_empty()
            || stdout.lines().count() <= 2,
        "should handle empty diff, got: {stdout}"
    );
}
