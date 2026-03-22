use crate::helpers::*;

// ---------------------------------------------------------------------------
// export formats
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn export_dotenv() {
    let bin = zuul_bin();
    let dir = setup_project("integ-export-dotenv");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "DB_URL", "postgres://local"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "API_KEY", "sk_test"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "-e", "dev", "--export-format", "dotenv"],
    );
    assert!(stdout.contains("DB_URL="));
    assert!(stdout.contains("API_KEY="));
}

#[test]
#[ignore = "needs emulator"]
fn export_json() {
    let bin = zuul_bin();
    let dir = setup_project("integ-export-json");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "value"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "-e", "dev", "--export-format", "json"],
    );
    let json = parse_json(&stdout);
    assert_eq!(json["KEY"].as_str().unwrap(), "value");
}

#[test]
#[ignore = "needs emulator"]
fn export_yaml() {
    let bin = zuul_bin();
    let dir = setup_project("integ-export-yaml");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "value"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "-e", "dev", "--export-format", "yaml"],
    );
    assert!(
        stdout.contains("KEY:"),
        "yaml should contain KEY:, got: {stdout}"
    );
}

#[test]
#[ignore = "needs emulator"]
fn export_direnv() {
    let bin = zuul_bin();
    let dir = setup_project("integ-export-direnv");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "value"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "-e", "dev", "--export-format", "direnv"],
    );
    assert!(
        stdout.contains("export KEY="),
        "direnv should have export statement, got: {stdout}"
    );
}

#[test]
#[ignore = "needs emulator"]
fn export_shell() {
    let bin = zuul_bin();
    let dir = setup_project("integ-export-shell");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "value"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "-e", "dev", "--export-format", "shell"],
    );
    assert!(
        stdout.contains("KEY=") || stdout.contains("KEY='"),
        "shell should contain KEY assignment, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// export to file
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn export_to_file() {
    let bin = zuul_bin();
    let dir = setup_project("integ-export-file");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "val"],
    );

    let output_file = dir.path().join("out.env");
    zuul_ok(
        bin,
        dir.path(),
        &[
            "export",
            "-e",
            "dev",
            "--export-format",
            "dotenv",
            "--output",
            output_file.to_str().unwrap(),
        ],
    );
    assert!(output_file.exists());
    let content = std::fs::read_to_string(&output_file).unwrap();
    assert!(content.contains("KEY="));
}

// ---------------------------------------------------------------------------
// export with local overrides
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn export_with_and_without_local_overrides() {
    let bin = zuul_bin();
    let dir = setup_project("integ-export-local");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "DB_URL", "remote://db"],
    );

    std::fs::write(
        dir.path().join(".zuul.local.toml"),
        "[secrets]\nDB_URL = \"local://db\"\n",
    )
    .unwrap();

    // Default: backend values only (no overrides)
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["export", "-e", "dev", "--export-format", "json"],
    );
    let json = parse_json(&stdout);
    assert_eq!(
        json["DB_URL"].as_str().unwrap(),
        "remote://db",
        "default should use backend value"
    );

    // --overrides: local override applied
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &[
            "export",
            "-e",
            "dev",
            "--export-format",
            "json",
            "--overrides",
        ],
    );
    let json = parse_json(&stdout);
    assert_eq!(
        json["DB_URL"].as_str().unwrap(),
        "local://db",
        "--overrides should apply local override"
    );
}

// ---------------------------------------------------------------------------
// import
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn import_dotenv() {
    let bin = zuul_bin();
    let dir = setup_project("integ-import-dotenv");

    create_envs(&dir, &["dev"]);

    let env_file = dir.path().join("test.env");
    std::fs::write(
        &env_file,
        "# comment\nDB_HOST=localhost\nDB_PORT=5432\nDB_NAME=mydb\n",
    )
    .unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &["import", "-e", "dev", "--file", env_file.to_str().unwrap()],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "DB_HOST"]);
    assert_eq!(stdout.trim(), "localhost");

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "DB_PORT"]);
    assert_eq!(stdout.trim(), "5432");
}

#[test]
#[ignore = "needs emulator"]
fn import_json() {
    let bin = zuul_bin();
    let dir = setup_project("integ-import-json");

    create_envs(&dir, &["dev"]);

    let json_file = dir.path().join("secrets.json");
    std::fs::write(
        &json_file,
        r#"{"REDIS_URL": "redis://localhost:6379", "TTL": "3600"}"#,
    )
    .unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &["import", "-e", "dev", "--file", json_file.to_str().unwrap()],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "-e", "dev", "REDIS_URL"],
    );
    assert_eq!(stdout.trim(), "redis://localhost:6379");
}

#[test]
#[ignore = "needs emulator"]
fn import_yaml() {
    let bin = zuul_bin();
    let dir = setup_project("integ-import-yaml");

    create_envs(&dir, &["dev"]);

    let yaml_file = dir.path().join("secrets.yaml");
    std::fs::write(&yaml_file, "APP_NAME: my-app\nDEBUG: \"true\"\n").unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &["import", "-e", "dev", "--file", yaml_file.to_str().unwrap()],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "APP_NAME"]);
    assert_eq!(stdout.trim(), "my-app");
}

#[test]
#[ignore = "needs emulator"]
fn import_dry_run() {
    let bin = zuul_bin();
    let dir = setup_project("integ-import-dry");

    create_envs(&dir, &["dev"]);

    let env_file = dir.path().join("test.env");
    std::fs::write(&env_file, "NEW_KEY=new_val\n").unwrap();

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &[
            "import",
            "-e",
            "dev",
            "--file",
            env_file.to_str().unwrap(),
            "--dry-run",
        ],
    );
    assert!(
        stdout.contains("NEW_KEY") || stdout.contains("dry") || stdout.contains("would"),
        "should preview import, got: {stdout}"
    );

    // Secret should NOT exist
    let stderr = zuul_err(bin, dir.path(), &["secret", "get", "-e", "dev", "NEW_KEY"]);
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "secret should not exist after dry run, got: {stderr}"
    );
}

#[test]
#[ignore = "needs emulator"]
fn import_skips_existing_by_default() {
    let bin = zuul_bin();
    let dir = setup_project("integ-import-skip");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "EXISTING", "original"],
    );

    let env_file = dir.path().join("test.env");
    std::fs::write(&env_file, "EXISTING=new_value\nFRESH=fresh_val\n").unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &["import", "-e", "dev", "--file", env_file.to_str().unwrap()],
    );

    // EXISTING should keep original value (skipped)
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "EXISTING"]);
    assert_eq!(stdout.trim(), "original", "existing should be skipped");

    // FRESH should be created
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "FRESH"]);
    assert_eq!(stdout.trim(), "fresh_val");
}

#[test]
#[ignore = "needs emulator"]
fn import_overwrite_replaces_existing() {
    let bin = zuul_bin();
    let dir = setup_project("integ-import-overwrite");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "KEY", "old"],
    );

    let env_file = dir.path().join("test.env");
    std::fs::write(&env_file, "KEY=new\n").unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &[
            "import",
            "-e",
            "dev",
            "--file",
            env_file.to_str().unwrap(),
            "--overwrite",
        ],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "KEY"]);
    assert_eq!(stdout.trim(), "new", "--overwrite should replace value");
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn run_injects_secrets() {
    let bin = zuul_bin();
    let dir = setup_project("integ-run-inject");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "MY_VAR", "hello_zuul"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["run", "-e", "dev", "--", "sh", "-c", "echo $MY_VAR"],
    );
    assert_eq!(stdout.trim(), "hello_zuul");
}

#[test]
#[ignore = "needs emulator"]
fn run_forwards_exit_code() {
    let bin = zuul_bin();
    let dir = setup_project("integ-run-exit");

    create_envs(&dir, &["dev"]);

    let output = zuul(
        bin,
        dir.path(),
        &["run", "-e", "dev", "--", "sh", "-c", "exit 42"],
    );
    assert_eq!(output.status.code(), Some(42));
}

#[test]
#[ignore = "needs emulator"]
fn run_overrides_applies_local_values() {
    let bin = zuul_bin();
    let dir = setup_project("integ-run-overrides");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "URL", "remote"],
    );

    std::fs::write(
        dir.path().join(".zuul.local.toml"),
        "[secrets]\nURL = \"local\"\n",
    )
    .unwrap();

    // Default: backend value (no overrides)
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["run", "-e", "dev", "--", "sh", "-c", "echo $URL"],
    );
    assert_eq!(stdout.trim(), "remote");

    // --overrides: local override applied
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &[
            "run",
            "-e",
            "dev",
            "--overrides",
            "--",
            "sh",
            "-c",
            "echo $URL",
        ],
    );
    assert_eq!(stdout.trim(), "local");
}

#[test]
#[ignore = "needs emulator"]
fn run_strips_zuul_env_vars() {
    let bin = zuul_bin();
    let dir = setup_project("integ-run-strip");

    create_envs(&dir, &["dev"]);

    // Set a ZUUL_ env var in the parent; it should NOT be passed to child
    let output = std::process::Command::new(bin)
        .args([
            "--non-interactive",
            "--no-color",
            "run",
            "-e",
            "dev",
            "--",
            "sh",
            "-c",
            "echo ZUUL_VAR=$ZUUL_DEFAULT_ENV",
        ])
        .current_dir(dir.path())
        .env("SECRET_MANAGER_EMULATOR_HOST", "http://localhost:9090")
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
// run collision warning on stderr
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn run_collision_warning_on_stderr() {
    let bin = zuul_bin();
    let dir = setup_project("integ-run-collision");

    create_envs(&dir, &["dev"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "-e", "dev", "HOME", "/zuul/home"],
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

// ---------------------------------------------------------------------------
// multiline value export round-trip
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn export_multiline_value_dotenv() {
    let bin = zuul_bin();
    let dir = setup_project("integ-export-multiline");

    create_envs(&dir, &["dev"]);

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
            "CERT",
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

    // Export as dotenv (should escape newlines)
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
// import format auto-detection
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn import_auto_detects_format_from_extension() {
    let bin = zuul_bin();
    let dir = setup_project("integ-import-autodetect");

    create_envs(&dir, &["dev"]);

    // .json extension -> auto-detect as JSON (no --import-format needed)
    let json_file = dir.path().join("data.json");
    std::fs::write(&json_file, r#"{"AUTO_KEY": "auto_val"}"#).unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &["import", "-e", "dev", "--file", json_file.to_str().unwrap()],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "AUTO_KEY"]);
    assert_eq!(stdout.trim(), "auto_val");

    // .yaml extension -> auto-detect as YAML
    let yaml_file = dir.path().join("data.yaml");
    std::fs::write(&yaml_file, "YAML_KEY: yaml_val\n").unwrap();

    zuul_ok(
        bin,
        dir.path(),
        &["import", "-e", "dev", "--file", yaml_file.to_str().unwrap()],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "-e", "dev", "YAML_KEY"]);
    assert_eq!(stdout.trim(), "yaml_val");
}
