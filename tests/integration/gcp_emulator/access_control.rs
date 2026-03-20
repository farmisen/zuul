use crate::helpers::*;

// ---------------------------------------------------------------------------
// Environment scoping: secrets are isolated between environments
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn secret_in_one_env_not_visible_in_another() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-env-scope");

    create_envs(&dir, &["dev", "staging"]);

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "dev", "DEV_SECRET", "dev_only"],
    );

    // Cannot get dev's secret from staging
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "get", "--env", "staging", "DEV_SECRET"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "dev secret should not be visible in staging, got: {stderr}"
    );
}

#[test]
#[ignore = "needs emulator"]
fn secret_delete_in_wrong_env_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-del-wrong-env");

    create_envs(&dir, &["dev", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "dev", "KEY", "val"],
    );

    // Deleting from wrong env should fail
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "delete", "--env", "staging", "KEY", "--force"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should not be able to delete from wrong env, got: {stderr}"
    );

    // Original should still exist
    let stdout = zuul_ok(bin, dir.path(), &["secret", "get", "--env", "dev", "KEY"]);
    assert_eq!(stdout.trim(), "val");
}

// ---------------------------------------------------------------------------
// Environment existence enforcement
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn operations_on_nonexistent_env_fail() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-no-env");

    create_envs(&dir, &["dev"]);

    // secret set in nonexistent env
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "set", "--env", "ghost", "KEY", "val"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("exist"),
        "set in missing env should fail, got: {stderr}"
    );

    // secret get in nonexistent env
    let stderr = zuul_err(bin, dir.path(), &["secret", "get", "--env", "ghost", "KEY"]);
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("exist"),
        "get in missing env should fail, got: {stderr}"
    );

    // export in nonexistent env
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["export", "--env", "ghost", "--export-format", "json"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("exist"),
        "export in missing env should fail, got: {stderr}"
    );

    // run in nonexistent env
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["run", "--env", "ghost", "--", "echo", "hi"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("exist"),
        "run in missing env should fail, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Secret existence enforcement
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn get_nonexistent_secret_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-no-secret-get");

    create_envs(&dir, &["dev"]);

    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "get", "--env", "dev", "DOES_NOT_EXIST"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report not found, got: {stderr}"
    );
}

#[test]
#[ignore = "needs emulator"]
fn info_nonexistent_secret_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-no-secret-info");

    create_envs(&dir, &["dev"]);

    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "info", "--env", "dev", "DOES_NOT_EXIST"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "should report not found, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Input validation (name constraints)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn secret_name_validation() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-secret-names");

    create_envs(&dir, &["dev"]);

    // Double underscore not allowed in secret names
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "set", "--env", "dev", "BAD__NAME", "val"],
    );
    assert!(
        stderr.contains("__") || stderr.contains("invalid") || stderr.contains("must"),
        "__ in secret name should be rejected, got: {stderr}"
    );

    // Names starting with a digit are not allowed
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "set", "--env", "dev", "1DIGIT", "val"],
    );
    assert!(
        stderr.contains("must start with") || stderr.contains("invalid") || stderr.contains("must"),
        "digit-starting secret name should be rejected, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Clearing one env does not affect other envs (isolation test)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn deleting_secret_in_one_env_does_not_affect_other() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-del-isolation");

    create_envs(&dir, &["alpha", "beta"]);

    // Same logical name in both envs, different values
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "alpha", "DB_URL", "alpha_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "beta", "DB_URL", "beta_val"],
    );

    // Delete from alpha
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "delete", "--env", "alpha", "DB_URL", "--force"],
    );

    // Beta should still have its secret
    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "--env", "beta", "DB_URL"],
    );
    assert_eq!(stdout.trim(), "beta_val");

    // Alpha should no longer have it
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["secret", "get", "--env", "alpha", "DB_URL"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "alpha secret should be gone, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// export / run on nonexistent env fails
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn export_nonexistent_env_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-export-ghost");

    let stderr = zuul_err(
        bin,
        dir.path(),
        &["export", "-e", "ghost", "--export-format", "json"],
    );
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("exist"),
        "export on nonexistent env should fail, got: {stderr}"
    );
}

#[test]
#[ignore = "needs emulator"]
fn run_nonexistent_env_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-run-ghost");

    let stderr = zuul_err(bin, dir.path(), &["run", "-e", "ghost", "--", "echo", "hi"]);
    assert!(
        stderr.contains("not") || stderr.contains("found") || stderr.contains("exist"),
        "run on nonexistent env should fail, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Cross-env secret list shows correct associations
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn secret_list_cross_env_shows_correct_envs() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-cross-list");

    create_envs(&dir, &["dev", "staging", "production"]);

    // Set SHARED in dev and production (not staging)
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "dev", "SHARED", "a"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "production", "SHARED", "b"],
    );

    let stdout = zuul_ok(bin, dir.path(), &["secret", "list"]);
    assert!(stdout.contains("SHARED"));
    assert!(
        stdout.contains("dev") && stdout.contains("production"),
        "should list environments, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Project isolation (different project IDs don't interfere)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn different_projects_are_isolated() {
    let bin = zuul_bin();

    let dir_a = setup_project("integ-ac-proj-a");
    let dir_b = setup_project("integ-ac-proj-b");

    create_envs(&dir_a, &["dev"]);
    zuul_ok(
        bin,
        dir_a.path(),
        &["secret", "set", "--env", "dev", "PROJ_KEY", "from_a"],
    );

    // Project B should have no environments
    let stdout = zuul_ok(bin, dir_b.path(), &["env", "list"]);
    assert!(
        !stdout.contains("dev") || stdout.trim().is_empty() || stdout.contains("No"),
        "project B should not see project A's envs, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Missing --env when no default configured
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs emulator"]
fn missing_env_with_no_default_fails() {
    let bin = zuul_bin();
    let dir = setup_project_no_default_env("integ-ac-no-default");

    create_envs(&dir, &["dev"]);

    let stderr = zuul_err(bin, dir.path(), &["secret", "set", "KEY", "val"]);
    assert!(
        stderr.contains("No environment")
            || stderr.contains("--env")
            || stderr.contains("environment"),
        "should ask for --env, got: {stderr}"
    );

    let stderr = zuul_err(bin, dir.path(), &["secret", "get", "KEY"]);
    assert!(
        stderr.contains("No environment")
            || stderr.contains("--env")
            || stderr.contains("environment"),
        "should ask for --env, got: {stderr}"
    );

    let stderr = zuul_err(bin, dir.path(), &["export", "--export-format", "json"]);
    assert!(
        stderr.contains("No environment")
            || stderr.contains("--env")
            || stderr.contains("environment"),
        "export should ask for --env, got: {stderr}"
    );
}
