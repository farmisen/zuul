use crate::helpers::*;

#[test]
#[ignore = "needs emulator"]
fn no_config_file_fails_with_message() {
    let bin = zuul_bin();
    let dir = tempfile::tempdir().unwrap();

    // No .zuul.toml in this directory
    let stderr = zuul_err(bin, dir.path(), &["env", "list"]);
    assert!(
        stderr.contains(".zuul.toml") || stderr.contains("zuul init") || stderr.contains("config"),
        "should mention missing config, got: {stderr}"
    );
}

#[test]
#[ignore = "needs emulator"]
fn missing_project_id_fails_with_message() {
    let bin = zuul_bin();
    let dir = tempfile::tempdir().unwrap();

    // Config without project_id
    std::fs::write(
        dir.path().join(".zuul.toml"),
        "[backend]\ntype = \"gcp-secret-manager\"\n",
    )
    .unwrap();

    let stderr = zuul_err(bin, dir.path(), &["env", "list"]);
    assert!(
        stderr.contains("project") || stderr.contains("Project") || stderr.contains("zuul init"),
        "should mention missing project ID, got: {stderr}"
    );
}

#[test]
#[ignore = "needs emulator"]
fn json_format_on_error() {
    let bin = zuul_bin();
    let dir = setup_project("integ-cfg-json-err");

    // Try to show a nonexistent env with JSON format.
    // Errors go to stderr as text (not JSON), even when --format json is set.
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["--format", "json", "env", "show", "nonexistent"],
    );
    assert!(
        stderr.contains("Error:") || stderr.contains("not") || stderr.contains("found"),
        "should produce an actionable error on stderr, got: {stderr}"
    );
}
