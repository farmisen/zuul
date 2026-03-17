use std::process;

use clap::Parser;
use rustls::crypto::ring::default_provider;

use zuul::backend::gcp::GcpClient;
use zuul::backend::gcp_backend::GcpBackend;
use zuul::cli::{
    Cli, Command, EnvCommand, MetadataCommand, RecoverCommand, SecretCommand, auth, diff, env,
    export, import, init, metadata, recover, run, secret,
};
use zuul::config::{CliOverrides, Config, load_config};
use zuul::error::ZuulError;
use zuul::progress::{BatchContext, ProgressOpts};

#[tokio::main]
async fn main() {
    default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let cli = Cli::parse();

    let result = run(cli).await;
    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn get_cwd() -> Result<std::path::PathBuf, ZuulError> {
    std::env::current_dir()
        .map_err(|e| ZuulError::Config(format!("Failed to get current directory: {e}")))
}

fn resolve_config(cli: &Cli, env: Option<&str>) -> Result<Config, ZuulError> {
    let cwd = get_cwd()?;
    load_config(
        &cwd,
        &CliOverrides {
            environment: env.map(String::from),
            project_id: cli.project.clone(),
            config_path: cli.config.clone(),
        },
    )
}

async fn create_backend(config: &Config) -> Result<GcpBackend, ZuulError> {
    let project_id = config.project_id.as_deref().ok_or_else(|| {
        ZuulError::Config(
            "No GCP project ID configured. Run 'zuul init' to set up your project.".to_string(),
        )
    })?;
    let client = GcpClient::new(project_id, config.credentials.as_deref()).await?;
    Ok(GcpBackend::new(client))
}

async fn run(cli: Cli) -> Result<(), ZuulError> {
    // Respect NO_COLOR env var (https://no-color.org) in addition to --no-color flag.
    if cli.no_color || std::env::var("NO_COLOR").is_ok() {
        console::set_colors_enabled(false);
        console::set_colors_enabled_stderr(false);
    }

    let progress = ProgressOpts {
        non_interactive: cli.non_interactive,
    };

    match cli.command {
        Command::Init { project, backend } => {
            let cwd = get_cwd()?;
            init::run(&cwd, project, &backend, cli.non_interactive)?;
        }
        Command::Auth { check } => {
            let config = resolve_config(&cli, None)?;
            auth::run(&config, check, cli.non_interactive).await?;
        }
        Command::Env { ref command } => {
            let config = resolve_config(&cli, None)?;
            let backend = create_backend(&config).await?;
            match command {
                EnvCommand::List => env::list(&backend, &cli.format).await?,
                EnvCommand::Show { name } => {
                    env::show(&backend, name, &cli.format).await?;
                }
                EnvCommand::Copy {
                    from,
                    to,
                    force,
                    dry_run,
                } => {
                    let ctx = BatchContext {
                        progress,
                        project_root: config.config_dir.clone(),
                    };
                    env::copy(&backend, from, to, *force, *dry_run, &cli.format, &ctx).await?;
                }
                EnvCommand::Clear {
                    name,
                    force,
                    dry_run,
                } => {
                    let ctx = BatchContext {
                        progress,
                        project_root: config.config_dir.clone(),
                    };
                    env::clear(&backend, name, *force, *dry_run, &cli.format, &ctx).await?;
                }
            }
        }
        Command::Secret { ref command } => match command {
            SecretCommand::List { env, with_metadata } => {
                let config = resolve_config(&cli, env.as_deref())?;
                let backend = create_backend(&config).await?;
                secret::list(
                    &backend,
                    env.as_deref(),
                    *with_metadata,
                    &cli.format,
                    progress,
                )
                .await?;
            }
            SecretCommand::Get { name, env } => {
                let config = resolve_config(&cli, env.as_deref())?;
                let backend = create_backend(&config).await?;
                let env = config.default_environment.as_deref();
                secret::get(&backend, name, env, progress).await?;
            }
            SecretCommand::Set {
                name,
                value,
                from_file,
                from_stdin,
                env,
            } => {
                let config = resolve_config(&cli, env.as_deref())?;
                let backend = create_backend(&config).await?;
                let env = config.default_environment.as_deref();
                secret::set(
                    &backend,
                    name,
                    env,
                    value.as_deref(),
                    from_file.as_deref(),
                    *from_stdin,
                    progress,
                )
                .await?;
            }
            SecretCommand::Delete {
                name,
                force,
                dry_run,
                env,
            } => {
                let config = resolve_config(&cli, env.as_deref())?;
                let backend = create_backend(&config).await?;
                let env = config.default_environment.as_deref();
                secret::delete(&backend, name, env, *force, *dry_run, &cli.format, progress)
                    .await?;
            }
            SecretCommand::Info { name, env } => {
                let config = resolve_config(&cli, env.as_deref())?;
                let backend = create_backend(&config).await?;
                let env = config.default_environment.as_deref();
                secret::info(&backend, name, env, &cli.format, progress).await?;
            }
            SecretCommand::Copy {
                name,
                from,
                to,
                force,
            } => {
                let config = resolve_config(&cli, None)?;
                let backend = create_backend(&config).await?;
                secret::copy(&backend, name, from, to, *force, progress).await?;
            }
            SecretCommand::Metadata { command: meta_cmd } => match meta_cmd {
                MetadataCommand::List { name, env } => {
                    let config = resolve_config(&cli, None)?;
                    let backend = create_backend(&config).await?;
                    metadata::list(&backend, name, env.as_deref(), &cli.format).await?;
                }
                MetadataCommand::Set {
                    name,
                    key,
                    value,
                    env,
                } => {
                    let config = resolve_config(&cli, None)?;
                    let backend = create_backend(&config).await?;
                    let ctx = BatchContext {
                        progress,
                        project_root: config.config_dir.clone(),
                    };
                    metadata::set(&backend, name, env.as_deref(), key, value, &ctx).await?;
                }
                MetadataCommand::Delete { name, key, env } => {
                    let config = resolve_config(&cli, None)?;
                    let backend = create_backend(&config).await?;
                    let ctx = BatchContext {
                        progress,
                        project_root: config.config_dir.clone(),
                    };
                    metadata::delete(&backend, name, env.as_deref(), key, &ctx).await?;
                }
            },
        },
        Command::Export {
            ref env,
            ref export_format,
            ref output,
            no_local,
        } => {
            let config = resolve_config(&cli, env.as_deref())?;
            let backend = create_backend(&config).await?;
            let env = secret::require_env(config.default_environment.as_deref())?;
            export::run(
                &backend,
                &config,
                env,
                export_format,
                output.as_deref(),
                no_local,
                progress,
            )
            .await?;
        }
        Command::Run {
            ref env,
            no_local,
            ref command,
        } => {
            let config = resolve_config(&cli, env.as_deref())?;
            let backend = create_backend(&config).await?;
            let env = secret::require_env(config.default_environment.as_deref())?;
            let exit_code = run::run(&backend, &config, env, no_local, command, progress).await?;
            process::exit(exit_code);
        }
        Command::Import {
            ref env,
            ref file,
            ref import_format,
            overwrite,
            dry_run,
        } => {
            let config = resolve_config(&cli, env.as_deref())?;
            let backend = create_backend(&config).await?;
            let env = secret::require_env(config.default_environment.as_deref())?;
            let ctx = BatchContext {
                progress,
                project_root: config.config_dir.clone(),
            };
            import::run(
                &backend,
                env,
                file,
                import_format.as_ref(),
                overwrite,
                dry_run,
                &ctx,
            )
            .await?;
        }
        Command::Diff {
            ref env_a,
            ref env_b,
            show_values,
        } => {
            let config = resolve_config(&cli, None)?;
            let backend = create_backend(&config).await?;
            diff::run(&backend, env_a, env_b, show_values, &cli.format, progress).await?;
        }
        Command::Recover { ref command } => {
            let config = resolve_config(&cli, None)?;
            let project_root = config.config_dir.as_deref().ok_or_else(|| {
                ZuulError::Config(
                    "No .zuul.toml found. Run 'zuul init' to set up your project.".to_string(),
                )
            })?;
            match command {
                RecoverCommand::Status => {
                    recover::status(project_root)?;
                }
                RecoverCommand::Resume { force } => {
                    let backend = create_backend(&config).await?;
                    recover::resume(
                        &backend,
                        project_root,
                        *force,
                        cli.non_interactive,
                        progress,
                    )
                    .await?;
                }
                RecoverCommand::Abort { force } => {
                    recover::abort(project_root, *force, cli.non_interactive)?;
                }
            }
        }
    }

    Ok(())
}
