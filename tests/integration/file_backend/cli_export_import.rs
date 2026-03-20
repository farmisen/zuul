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

// ---------------------------------------------------------------------------
// export --format shell
// ---------------------------------------------------------------------------

#[test]
fn export_shell() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "value"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "--env", "dev", "--export-format", "shell"],
    );
    assert!(
        stdout.contains("KEY=") || stdout.contains("KEY='"),
        "shell should contain KEY assignment, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// export multiline value in dotenv format
// ---------------------------------------------------------------------------

#[test]
fn export_multiline_value_dotenv() {
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
            "CERT",
            "-e",
            "dev",
            "--from-file",
            file_path.to_str().unwrap(),
        ],
    );

    // Export as JSON (most reliable for value verification)
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "-e", "dev", "--export-format", "json"],
    );
    let json = parse_json(&stdout);
    assert_eq!(
        json["CERT"].as_str().unwrap(),
        multiline,
        "JSON export should preserve multiline value"
    );

    // Export as dotenv (should contain CERT)
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "-e", "dev", "--export-format", "dotenv"],
    );
    assert!(
        stdout.contains("CERT="),
        "dotenv should contain CERT, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// export with and without local overrides
// ---------------------------------------------------------------------------

#[test]
fn export_with_and_without_local_overrides() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "DB_URL", "-e", "dev", "remote://db"],
    );

    std::fs::write(
        dir.path().join(".zuul.local.toml"),
        "[secrets]\nDB_URL = \"local://db\"\n",
    )
    .unwrap();

    // Default: local overrides applied
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "-e", "dev", "--export-format", "json"],
    );
    let json = parse_json(&stdout);
    assert_eq!(
        json["DB_URL"].as_str().unwrap(),
        "local://db",
        "local override should win"
    );

    // --no-local: backend value
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &[
            "export",
            "-e",
            "dev",
            "--export-format",
            "json",
            "--no-local",
        ],
    );
    let json = parse_json(&stdout);
    assert_eq!(
        json["DB_URL"].as_str().unwrap(),
        "remote://db",
        "--no-local should use backend value"
    );
}

// ---------------------------------------------------------------------------
// import YAML
// ---------------------------------------------------------------------------

#[test]
fn import_yaml() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    let yaml_file = dir.path().join("secrets.yaml");
    std::fs::write(&yaml_file, "APP_NAME: my-app\nDEBUG: \"true\"\n").unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "import",
            "--env",
            "dev",
            "--file",
            yaml_file.to_str().unwrap(),
        ],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "APP_NAME", "-e", "dev"]);
    assert_eq!(stdout.trim(), "my-app");
}

// ---------------------------------------------------------------------------
// import skips existing by default
// ---------------------------------------------------------------------------

#[test]
fn import_skips_existing_by_default() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "EXISTING", "-e", "dev", "original"],
    );

    let env_file = dir.path().join("test.env");
    std::fs::write(&env_file, "EXISTING=new_value\nFRESH=fresh_val\n").unwrap();

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

    // EXISTING should keep original value (skipped)
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "EXISTING", "-e", "dev"]);
    assert_eq!(stdout.trim(), "original", "existing should be skipped");

    // FRESH should be created
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "FRESH", "-e", "dev"]);
    assert_eq!(stdout.trim(), "fresh_val");
}

// ---------------------------------------------------------------------------
// import auto-detects format from extension
// ---------------------------------------------------------------------------

#[test]
fn import_auto_detects_format_from_extension() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    // .json extension -> auto-detect as JSON (no --import-format needed)
    let json_file = dir.path().join("data.json");
    std::fs::write(&json_file, r#"{"AUTO_KEY": "auto_val"}"#).unwrap();

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

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "AUTO_KEY", "-e", "dev"]);
    assert_eq!(stdout.trim(), "auto_val");

    // .yaml extension -> auto-detect as YAML
    let yaml_file = dir.path().join("data.yaml");
    std::fs::write(&yaml_file, "YAML_KEY: yaml_val\n").unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "import",
            "--env",
            "dev",
            "--file",
            yaml_file.to_str().unwrap(),
        ],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "YAML_KEY", "-e", "dev"]);
    assert_eq!(stdout.trim(), "yaml_val");
}

// ---------------------------------------------------------------------------
// run strips ZUUL_* env vars
// ---------------------------------------------------------------------------

#[test]
fn run_strips_zuul_env_vars() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    // Set a ZUUL_ env var in the parent; it should NOT be passed to child
    let output = std::process::Command::new(bin)
        .args([
            "--non-interactive",
            "--no-color",
            "run",
            "--env",
            "dev",
            "--",
            "sh",
            "-c",
            "echo ZUUL_VAR=$ZUUL_DEFAULT_ENV",
        ])
        .current_dir(dir.path())
        .env("ZUUL_PASSPHRASE", "test-passphrase")
        .env("ZUUL_DEFAULT_ENV", "should_be_stripped")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("should_be_stripped"),
        "ZUUL_ vars should be stripped from child, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// run --no-local skips overrides
// ---------------------------------------------------------------------------

#[test]
fn run_no_local_skips_overrides() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "URL", "-e", "dev", "remote"],
    );

    std::fs::write(
        dir.path().join(".zuul.local.toml"),
        "[secrets]\nURL = \"local\"\n",
    )
    .unwrap();

    // Default: local override applied
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["run", "-e", "dev", "--", "sh", "-c", "echo $URL"],
    );
    assert_eq!(stdout.trim(), "local");

    // --no-local: backend value
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &[
            "run",
            "-e",
            "dev",
            "--no-local",
            "--",
            "sh",
            "-c",
            "echo $URL",
        ],
    );
    assert_eq!(stdout.trim(), "remote");
}

// ---------------------------------------------------------------------------
// run collision warning on stderr
// ---------------------------------------------------------------------------

#[test]
fn run_collision_warning_on_stderr() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "HOME", "-e", "dev", "/zuul/home"],
    );

    // HOME is a common env var that will collide. The secret should win
    // and a warning should appear on stderr.
    let output = zuul(
        bin,
        dir.path(),
        &["run", "-e", "dev", "--", "sh", "-c", "echo $HOME"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success());
    assert_eq!(
        stdout.trim(),
        "/zuul/home",
        "secret should override env var"
    );
    assert!(
        stderr.contains("Warning") || stderr.contains("override"),
        "should warn about collision on stderr, got stderr: {stderr}"
    );
}
