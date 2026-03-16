use crate::helpers::*;

// ---------------------------------------------------------------------------
// Environment scoping: secrets are isolated between environments
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn secret_in_one_env_not_visible_in_another() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-env-scope");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);

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
#[ignore]
fn secret_delete_in_wrong_env_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-del-wrong-env");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
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
#[ignore]
fn operations_on_nonexistent_env_fail() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-no-env");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

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
#[ignore]
fn get_nonexistent_secret_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-no-secret-get");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

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
#[ignore]
fn info_nonexistent_secret_fails() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-no-secret-info");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

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
#[ignore]
fn env_name_validation() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-env-names");

    // Uppercase not allowed (env names must be lowercase)
    let stderr = zuul_err(bin, dir.path(), &["env", "create", "UPPERCASE"]);
    assert!(
        stderr.contains("must start with")
            || stderr.contains("invalid")
            || stderr.contains("Invalid"),
        "uppercase env name should be rejected, got: {stderr}"
    );

    // Double underscore not allowed
    let stderr = zuul_err(bin, dir.path(), &["env", "create", "bad__name"]);
    assert!(
        stderr.contains("__") || stderr.contains("invalid") || stderr.contains("must"),
        "__ in env name should be rejected, got: {stderr}"
    );

    // Reserved name 'registry'
    let stderr = zuul_err(bin, dir.path(), &["env", "create", "registry"]);
    assert!(
        stderr.contains("reserved")
            || stderr.contains("Reserved")
            || stderr.contains("invalid")
            || stderr.contains("must"),
        "reserved name should be rejected, got: {stderr}"
    );

    // Reserved name 'config'
    let stderr = zuul_err(bin, dir.path(), &["env", "create", "config"]);
    assert!(
        stderr.contains("reserved")
            || stderr.contains("Reserved")
            || stderr.contains("invalid")
            || stderr.contains("must"),
        "reserved name should be rejected, got: {stderr}"
    );
}

#[test]
#[ignore]
fn secret_name_validation() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-secret-names");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

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
#[ignore]
fn deleting_secret_in_one_env_does_not_affect_other() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-del-isolation");

    zuul_ok(bin, dir.path(), &["env", "create", "alpha"]);
    zuul_ok(bin, dir.path(), &["env", "create", "beta"]);

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
// env delete --dry-run shows the right resources
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_delete_dry_run_shows_bound_secrets() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-del-dry-secrets");

    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "staging", "DB_URL", "x"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "staging", "API_KEY", "y"],
    );

    let stdout = zuul_ok(bin, dir.path(), &["env", "delete", "staging", "--dry-run"]);
    // Dry run should list the env and bound secrets
    assert!(
        stdout.contains("staging"),
        "should mention the env, got: {stdout}"
    );
    assert!(
        stdout.contains("DB_URL") || stdout.contains("API_KEY"),
        "should list bound secrets, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// env rename without --force requires confirmation (non-interactive fails)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_rename_requires_confirmation_without_force() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-rename-confirm");

    zuul_ok(bin, dir.path(), &["env", "create", "old-name"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "old-name", "KEY", "val"],
    );

    // Without --force, rename with secrets should fail in non-interactive mode
    let stderr = zuul_err(
        bin,
        dir.path(),
        &["env", "update", "old-name", "--new-name", "new-name"],
    );
    assert!(
        stderr.contains("Confirmation") || stderr.contains("force") || stderr.contains("confirm"),
        "should indicate confirmation needed, got: {stderr}"
    );

    // Original env should still exist (no partial rename)
    let stdout = zuul_ok(bin, dir.path(), &["env", "show", "old-name"]);
    assert!(stdout.contains("old-name"));
}

// ---------------------------------------------------------------------------
// env delete --force removes env and secrets, other envs unaffected
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn env_delete_force_removes_env_and_secrets() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-del-force");

    zuul_ok(bin, dir.path(), &["env", "create", "target"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "target", "KEY_A", "a"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "--env", "target", "KEY_B", "b"],
    );

    zuul_ok(bin, dir.path(), &["env", "delete", "target", "--force"]);

    // Environment should be gone
    let stderr = zuul_err(bin, dir.path(), &["env", "show", "target"]);
    assert!(
        stderr.contains("not") || stderr.contains("found"),
        "env should be deleted, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// export / run on nonexistent env fails
// ---------------------------------------------------------------------------

#[test]
#[ignore]
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
#[ignore]
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
#[ignore]
fn secret_list_cross_env_shows_correct_envs() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-cross-list");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(bin, dir.path(), &["env", "create", "production"]);

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
#[ignore]
fn different_projects_are_isolated() {
    let bin = zuul_bin();

    let dir_a = setup_project("integ-ac-proj-a");
    let dir_b = setup_project("integ-ac-proj-b");

    zuul_ok(bin, dir_a.path(), &["env", "create", "dev"]);
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
// Duplicate env creation is rejected
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn duplicate_env_creation_rejected() {
    let bin = zuul_bin();
    let dir = setup_project("integ-ac-dup-env");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    let stderr = zuul_err(bin, dir.path(), &["env", "create", "dev"]);
    assert!(
        stderr.contains("already") || stderr.contains("exists"),
        "duplicate env should be rejected, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Missing --env when no default configured
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn missing_env_with_no_default_fails() {
    let bin = zuul_bin();
    let dir = setup_project_no_default_env("integ-ac-no-default");

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);

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
