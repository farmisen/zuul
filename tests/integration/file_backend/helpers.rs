use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

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

/// Run a zuul command in a given working directory with ZUUL_PASSPHRASE set.
pub fn zuul(bin: &str, work_dir: &Path, args: &[&str]) -> Output {
    let mut full_args = vec!["--non-interactive", "--no-color"];
    full_args.extend_from_slice(args);
    Command::new(bin)
        .args(&full_args)
        .current_dir(work_dir)
        .env("ZUUL_PASSPHRASE", "test-passphrase")
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
        .env("ZUUL_PASSPHRASE", "test-passphrase")
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

/// Create a temp dir with a `.zuul.toml` for the file backend.
pub fn setup_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let config = "[backend]\ntype = \"file\"\n\n[defaults]\nenvironment = \"dev\"\n";
    std::fs::write(dir.path().join(".zuul.toml"), config).expect("failed to write .zuul.toml");

    // Create the empty encrypted store so the backend works without `zuul init`.
    let bin = zuul_bin();
    // We can't easily create the store without the CLI, so just let the first
    // write operation create it implicitly (FileBackend handles missing files).
    // But we need the env to exist, so we'll create it in each test.
    let _ = bin; // ensure binary is built

    dir
}

/// Create a temp dir with a file backend and a "dev" environment pre-created.
pub fn setup_project_with_env() -> tempfile::TempDir {
    let dir = setup_project();
    let bin = zuul_bin();
    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    dir
}

/// Parse a JSON string into a serde_json::Value.
pub fn parse_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("Failed to parse JSON: {e}\nInput: {s}"))
}
