use clap::Parser;
use zuul::cli::{Cli, Command, EnvCommand, ExportFormat, SecretCommand};

// Note: EnvCommand::Create, Update, Delete were removed in item 5.14
// (environment lifecycle moved to Terraform).

/// Helper to parse args, prepending the binary name.
fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["zuul"];
    full.extend_from_slice(args);
    Cli::parse_from(full)
}

#[test]
fn init_default() {
    let cli = parse(&["init"]);
    assert!(matches!(cli.command, Command::Init { .. }));
}

#[test]
fn init_with_project() {
    let cli = parse(&["init", "--project", "my-proj"]);
    match cli.command {
        Command::Init { project, .. } => assert_eq!(project.as_deref(), Some("my-proj")),
        _ => panic!("expected Init"),
    }
}

#[test]
fn auth_check() {
    let cli = parse(&["auth", "--check"]);
    match cli.command {
        Command::Auth { check } => assert!(check),
        _ => panic!("expected Auth"),
    }
}

#[test]
fn env_list() {
    let cli = parse(&["env", "list"]);
    assert!(matches!(
        cli.command,
        Command::Env {
            command: EnvCommand::List
        }
    ));
}

#[test]
fn env_clear() {
    let cli = parse(&["env", "clear", "staging", "--force"]);
    match cli.command {
        Command::Env {
            command:
                EnvCommand::Clear {
                    name,
                    force,
                    dry_run,
                },
        } => {
            assert_eq!(name, "staging");
            assert!(force);
            assert!(!dry_run);
        }
        _ => panic!("expected Env Clear"),
    }
}

#[test]
fn secret_get() {
    let cli = parse(&["secret", "get", "DB_URL", "--env", "prod"]);
    match cli.command {
        Command::Secret {
            command: SecretCommand::Get { name, env },
        } => {
            assert_eq!(env.as_deref(), Some("prod"));
            assert_eq!(name, "DB_URL");
        }
        _ => panic!("expected Secret Get"),
    }
}

#[test]
fn secret_set_with_value() {
    let cli = parse(&["secret", "set", "API_KEY", "secret123", "--env", "dev"]);
    match cli.command {
        Command::Secret {
            command: SecretCommand::Set {
                name, value, env, ..
            },
        } => {
            assert_eq!(env.as_deref(), Some("dev"));
            assert_eq!(name, "API_KEY");
            assert_eq!(value.as_deref(), Some("secret123"));
        }
        _ => panic!("expected Secret Set"),
    }
}

#[test]
fn secret_copy() {
    let cli = parse(&[
        "secret", "copy", "DB_URL", "--from", "dev", "--to", "staging",
    ]);
    match cli.command {
        Command::Secret {
            command: SecretCommand::Copy { name, from, to, .. },
            ..
        } => {
            assert_eq!(name, "DB_URL");
            assert_eq!(from, "dev");
            assert_eq!(to, "staging");
        }
        _ => panic!("expected Secret Copy"),
    }
}

#[test]
fn export_with_format() {
    let cli = parse(&["export", "--export-format", "dotenv", "--env", "dev"]);
    match cli.command {
        Command::Export {
            env, export_format, ..
        } => {
            assert_eq!(env.as_deref(), Some("dev"));
            assert!(matches!(export_format, ExportFormat::Dotenv));
        }
        _ => panic!("expected Export"),
    }
}

#[test]
fn run_with_command() {
    let cli = parse(&["run", "--env", "prod", "--", "node", "server.js"]);
    match cli.command {
        Command::Run { env, command, .. } => {
            assert_eq!(env.as_deref(), Some("prod"));
            assert_eq!(command, vec!["node", "server.js"]);
        }
        _ => panic!("expected Run"),
    }
}

#[test]
fn import_with_options() {
    let cli = parse(&[
        "import",
        "--env",
        "dev",
        "--file",
        ".env",
        "--overwrite",
        "--dry-run",
    ]);
    match cli.command {
        Command::Import {
            file,
            overwrite,
            dry_run,
            ..
        } => {
            assert_eq!(file.to_str().unwrap(), ".env");
            assert!(overwrite);
            assert!(dry_run);
        }
        _ => panic!("expected Import"),
    }
}

#[test]
fn global_flags() {
    let cli = parse(&[
        "--project",
        "my-proj",
        "--non-interactive",
        "-v",
        "env",
        "list",
    ]);
    assert_eq!(cli.project.as_deref(), Some("my-proj"));
    assert!(cli.non_interactive);
    assert!(cli.verbose);
}

#[test]
fn diff_basic() {
    let cli = parse(&["diff", "dev", "staging"]);
    match cli.command {
        Command::Diff {
            env_a,
            env_b,
            show_values,
        } => {
            assert_eq!(env_a, "dev");
            assert_eq!(env_b, "staging");
            assert!(!show_values);
        }
        _ => panic!("expected Diff"),
    }
}

#[test]
fn diff_show_values() {
    let cli = parse(&["diff", "dev", "production", "--show-values"]);
    match cli.command {
        Command::Diff {
            env_a,
            env_b,
            show_values,
        } => {
            assert_eq!(env_a, "dev");
            assert_eq!(env_b, "production");
            assert!(show_values);
        }
        _ => panic!("expected Diff"),
    }
}
