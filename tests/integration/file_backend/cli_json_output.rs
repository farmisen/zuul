//! Tests for JSON output coverage across commands.

use crate::helpers::*;

// ---------------------------------------------------------------------------
// diff --format json with differences
// ---------------------------------------------------------------------------

#[test]
fn diff_json_with_differences() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "dev_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "staging", "stg_val"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["diff", "dev", "staging", "--format", "json"],
    );
    let json = parse_json(&stdout);
    let arr = json.as_array().expect("diff json should be an array");
    assert!(!arr.is_empty(), "should have at least one diff entry");

    let entry = &arr[0];
    assert_eq!(entry["name"], "KEY");
    assert_eq!(entry["status"], "differs");
    // Values should not be present without --show-values
    assert!(
        entry.get("value_a").is_none(),
        "values should not be present without --show-values"
    );
}

// ---------------------------------------------------------------------------
// diff --format json --show-values reveals values
// ---------------------------------------------------------------------------

#[test]
fn diff_json_show_values() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "dev_val"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "staging", "stg_val"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &[
            "diff",
            "dev",
            "staging",
            "--show-values",
            "--format",
            "json",
        ],
    );
    let json = parse_json(&stdout);
    let arr = json.as_array().expect("diff json should be an array");
    let entry = &arr[0];
    assert_eq!(entry["name"], "KEY");
    assert_eq!(entry["value_a"], "dev_val");
    assert_eq!(entry["value_b"], "stg_val");
}

// ---------------------------------------------------------------------------
// diff --format json with no differences
// ---------------------------------------------------------------------------

#[test]
fn diff_json_no_differences() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "dev", "same"],
    );
    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "KEY", "-e", "staging", "same"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["diff", "dev", "staging", "--format", "json"],
    );
    let json = parse_json(&stdout);
    let arr = json.as_array().expect("diff json should be an array");
    // Either empty array (no diffs) or entries with status "equal"
    for entry in arr {
        assert_eq!(
            entry["status"], "equal",
            "identical secrets should have status 'equal'"
        );
    }
}

// ---------------------------------------------------------------------------
// diff --format json with empty envs
// ---------------------------------------------------------------------------

#[test]
fn diff_json_empty_envs() {
    let dir = setup_project();
    let bin = zuul_bin();

    zuul_ok(bin, dir.path(), &["env", "create", "dev"]);
    zuul_ok(bin, dir.path(), &["env", "create", "staging"]);

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["diff", "dev", "staging", "--format", "json"],
    );
    let json = parse_json(&stdout);
    let arr = json.as_array().expect("diff json should be an array");
    assert!(arr.is_empty(), "empty envs should produce empty array");
}

// ---------------------------------------------------------------------------
// secret get returns raw value (no format flag)
// ---------------------------------------------------------------------------

#[test]
fn secret_get_returns_raw_value() {
    let dir = setup_project_with_env();
    let bin = zuul_bin();

    zuul_ok(
        bin,
        dir.path(),
        &["secret", "set", "MY_SECRET", "-e", "dev", "secret_value"],
    );

    let stdout = zuul_ok(
        bin,
        dir.path(),
        &["secret", "get", "MY_SECRET", "-e", "dev"],
    );
    assert_eq!(
        stdout.trim(),
        "secret_value",
        "secret get should return the raw value"
    );
}
