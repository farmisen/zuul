use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

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
        .env("SECRET_MANAGER_EMULATOR_HOST", "http://localhost:9090")
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

/// Run a zuul command with data piped to stdin.
pub fn zuul_stdin(bin: &str, work_dir: &Path, args: &[&str], input: &str) -> Output {
    use std::io::Write;
    let mut full_args = vec!["--non-interactive", "--no-color"];
    full_args.extend_from_slice(args);
    let mut child = Command::new(bin)
        .args(&full_args)
        .current_dir(work_dir)
        .env("SECRET_MANAGER_EMULATOR_HOST", "http://localhost:9090")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn zuul {}: {e}", args.join(" ")));
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .expect("failed to write stdin");
    child.wait_with_output().expect("failed to wait for zuul")
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

/// Create a temp dir with a `.zuul.toml` that has NO default environment.
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

/// Parse a string as JSON.
pub fn parse_json(output: &str) -> serde_json::Value {
    serde_json::from_str(output).unwrap_or_else(|e| panic!("invalid JSON: {e}\n---\n{output}"))
}
