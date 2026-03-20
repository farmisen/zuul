use crate::helpers::*;

#[test]
#[ignore = "needs emulator"]
fn auth_check_succeeds_against_emulator() {
    let bin = zuul_bin();
    let dir = setup_project("integ-auth-check");

    // --check against emulator should succeed (emulator skips real auth)
    let stdout = zuul_ok(bin, dir.path(), &["auth", "--check"]);
    let _ = stdout; // exit code 0 is the success signal
}

#[test]
#[ignore = "needs emulator"]
fn auth_check_no_config_fails() {
    let bin = zuul_bin();
    let dir = tempfile::tempdir().unwrap();

    // No .zuul.toml → auth should fail
    let stderr = zuul_err(bin, dir.path(), &["auth", "--check"]);
    assert!(
        stderr.contains("zuul.toml")
            || stderr.contains("zuul init")
            || stderr.contains("config")
            || stderr.contains("backend"),
        "should report missing config, got: {stderr}"
    );
}
