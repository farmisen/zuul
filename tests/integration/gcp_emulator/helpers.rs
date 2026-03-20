use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use base64::Engine;

/// Monotonic counter to make project IDs unique across tests within one run.
static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Build the zuul binary once (shared across all tests) and return the path.
pub fn zuul_bin() -> &'static str {
    static BIN_PATH: OnceLock<String> = OnceLock::new();
    BIN_PATH.get_or_init(|| {
        let output = Command::new("cargo")
            .args(["build", "--quiet"])
            .output()
            .expect("failed to build zuul");
        assert!(
            output.status.success(),
            "cargo build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let metadata = Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .output()
            .expect("failed to get cargo metadata");
        let meta: serde_json::Value =
            serde_json::from_slice(&metadata.stdout).expect("failed to parse metadata");
        let target_dir = meta["target_directory"].as_str().unwrap();
        format!("{target_dir}/debug/zuul")
    })
}

/// Run a zuul command in a given working directory with emulator env set.
/// Always passes `--non-interactive` and `--no-color` for predictable output.
pub fn zuul(bin: &str, work_dir: &Path, args: &[&str]) -> Output {
    let mut full_args = vec!["--non-interactive", "--no-color"];
    full_args.extend_from_slice(args);
    Command::new(bin)
        .args(&full_args)
        .current_dir(work_dir)
        .env("SECRET_MANAGER_EMULATOR_HOST", "http://localhost:8080")
        .output()
        .unwrap_or_else(|e| panic!("failed to run zuul {}: {e}", args.join(" ")))
}

/// Run a zuul command and assert it succeeded, returning stdout.
pub fn zuul_ok(bin: &str, work_dir: &Path, args: &[&str]) -> String {
    let output = zuul(bin, work_dir, args);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "zuul {} failed (exit {}):\nstdout: {stdout}\nstderr: {stderr}",
        args.join(" "),
        output.status.code().unwrap_or(-1),
    );
    stdout
}

/// Run a zuul command and assert it failed, returning stderr.
pub fn zuul_err(bin: &str, work_dir: &Path, args: &[&str]) -> String {
    let output = zuul(bin, work_dir, args);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        !output.status.success(),
        "zuul {} should have failed but succeeded:\nstdout: {stdout}\nstderr: {stderr}",
        args.join(" "),
    );
    stderr
}

/// Run a zuul command with stdin data piped in.
pub fn zuul_stdin(bin: &str, work_dir: &Path, args: &[&str], stdin_data: &str) -> Output {
    let mut full_args = vec!["--non-interactive", "--no-color"];
    full_args.extend_from_slice(args);
    let mut child = Command::new(bin)
        .args(&full_args)
        .current_dir(work_dir)
        .env("SECRET_MANAGER_EMULATOR_HOST", "http://localhost:8080")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn zuul {}: {e}", args.join(" ")));

    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(stdin_data.as_bytes()).unwrap();
    }
    child
        .wait_with_output()
        .unwrap_or_else(|e| panic!("failed to wait on zuul {}: {e}", args.join(" ")))
}

/// Generate a unique project ID to prevent collisions across test runs.
/// Appends a per-process monotonic counter to the base name.
fn unique_project_id(base: &str) -> String {
    let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    format!("{base}-{pid}-{n}")
}

/// Create a temp dir with a `.zuul.toml` pointing at a test project.
/// The project_id is made unique per run to avoid emulator state collisions.
pub fn setup_project(project_id: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let uid = unique_project_id(project_id);
    let config = format!(
        r#"[backend]
type = "gcp-secret-manager"
project_id = "{uid}"

[defaults]
environment = "dev"
"#
    );
    std::fs::write(dir.path().join(".zuul.toml"), config).expect("failed to write .zuul.toml");
    dir
}

/// Create a temp dir with a `.zuul.toml` that has no default environment.
pub fn setup_project_no_default_env(project_id: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let uid = unique_project_id(project_id);
    let config = format!(
        r#"[backend]
type = "gcp-secret-manager"
project_id = "{uid}"
"#
    );
    std::fs::write(dir.path().join(".zuul.toml"), config).expect("failed to write .zuul.toml");
    dir
}

/// Parse a JSON string, panicking with a helpful message on failure.
pub fn parse_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("failed to parse JSON: {e}\ninput: {s}"))
}

/// Create environments by writing the `zuul__registry` secret directly to the
/// emulator REST API. This bypasses `zuul env create` (which is blocked for GCP).
///
/// Reads the `.zuul.toml` from the temp dir to extract the actual project_id
/// (which includes a unique suffix from `setup_project`).
pub fn create_envs(project_id_from_setup: &tempfile::TempDir, envs: &[&str]) {
    // Read .zuul.toml to extract the project_id with unique suffix
    let config_path = project_id_from_setup.path().join(".zuul.toml");
    let config_content = std::fs::read_to_string(&config_path).expect("failed to read .zuul.toml");
    let project_id = config_content
        .lines()
        .find(|line| line.starts_with("project_id"))
        .and_then(|line| {
            let value = line.split('=').nth(1)?.trim();
            Some(value.trim_matches('"').to_string())
        })
        .expect("failed to extract project_id from .zuul.toml");

    // Build registry JSON
    let mut env_entries = Vec::new();
    for env_name in envs {
        env_entries.push(format!(
            r#""{env_name}":{{"description":"","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}}"#
        ));
    }
    let registry_json = format!(
        r#"{{"version":1,"environments":{{{}}}}}"#,
        env_entries.join(",")
    );

    let base_url = "http://localhost:8080/v1";

    // Create the zuul__registry secret
    let output = Command::new("curl")
        .args([
            "-s",
            "-X",
            "POST",
            &format!("{base_url}/projects/{project_id}/secrets?secretId=zuul__registry"),
            "-H",
            "Content-Type: application/json",
            "-d",
            r#"{"replication":{"automatic":{}}}"#,
        ])
        .output()
        .expect("failed to run curl to create registry secret");
    assert!(
        output.status.success(),
        "curl create secret failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Add a version with the registry data
    let encoded = base64::engine::general_purpose::STANDARD.encode(registry_json.as_bytes());
    let version_body = format!(r#"{{"payload":{{"data":"{encoded}"}}}}"#);

    let output = Command::new("curl")
        .args([
            "-s",
            "-X",
            "POST",
            &format!("{base_url}/projects/{project_id}/secrets/zuul__registry:addVersion"),
            "-H",
            "Content-Type: application/json",
            "-d",
            &version_body,
        ])
        .output()
        .expect("failed to run curl to add registry version");
    assert!(
        output.status.success(),
        "curl add version failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
