use std::process;

use clap::Parser;

use zuul::backend::gcp::GcpClient;
use zuul::backend::gcp_backend::GcpBackend;
use zuul::cli::{
    Cli, Command, EnvCommand, MetadataCommand, SecretCommand, auth, env, export, import, init,
    metadata, run, secret,
};
use zuul::config::{CliOverrides, Config, load_config};
use zuul::error::ZuulError;

#[tokio::main]
async fn main() {
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

fn resolve_config(cli: &Cli) -> Result<Config, ZuulError> {
    let cwd = get_cwd()?;
    load_config(
        &cwd,
        &CliOverrides {
            environment: cli.env.clone(),
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
    match cli.command {
        Command::Init { project, backend } => {
            let cwd = get_cwd()?;
            init::run(&cwd, project, &backend)?;
        }
        Command::Auth { check } => {
            let config = resolve_config(&cli)?;
            auth::run(&config, check).await?;
        }
        Command::Env { ref command } => {
            let config = resolve_config(&cli)?;
            let backend = create_backend(&config).await?;
            match command {
                EnvCommand::List => env::list(&backend, &cli.format).await?,
                EnvCommand::Create { name, description } => {
                    env::create(&backend, name, description.as_deref(), &cli.format).await?;
                }
                EnvCommand::Show { name } => {
                    env::show(&backend, name, &cli.format).await?;
                }
                EnvCommand::Update {
                    name,
                    new_name,
                    description,
                } => {
                    env::update(
                        &backend,
                        name,
                        new_name.as_deref(),
                        description.as_deref(),
                        &cli.format,
                    )
                    .await?;
                }
                EnvCommand::Delete { name, dry_run } => {
                    env::delete(&backend, name, *dry_run, &cli.format).await?;
                }
            }
        }
        Command::Secret { ref command } => {
            let config = resolve_config(&cli)?;
            let backend = create_backend(&config).await?;
            let env = config.default_environment.as_deref();
            match command {
                SecretCommand::List => {
                    secret::list(&backend, env, &cli.format).await?;
                }
                SecretCommand::Get { name } => {
                    secret::get(&backend, name, env).await?;
                }
                SecretCommand::Set {
                    name,
                    value,
                    from_file,
                    from_stdin,
                } => {
                    secret::set(
                        &backend,
                        name,
                        env,
                        value.as_deref(),
                        from_file.as_deref(),
                        *from_stdin,
                        cli.quiet,
                    )
                    .await?;
                }
                SecretCommand::Delete {
                    name,
                    force,
                    dry_run,
                } => {
                    secret::delete(&backend, name, env, *force, *dry_run, &cli.format).await?;
                }
                SecretCommand::Info { name } => {
                    secret::info(&backend, name, env, &cli.format).await?;
                }
                SecretCommand::Copy {
                    name,
                    from,
                    to,
                    force,
                } => {
                    secret::copy(&backend, name, from, to, *force, cli.quiet).await?;
                }
                SecretCommand::Metadata { command: meta_cmd } => match meta_cmd {
                    MetadataCommand::List { name } => {
                        metadata::list(&backend, name, env, &cli.format).await?;
                    }
                    MetadataCommand::Set { name, key, value } => {
                        metadata::set(&backend, name, env, key, value, cli.quiet).await?;
                    }
                    MetadataCommand::Delete { name, key } => {
                        metadata::delete(&backend, name, env, key, cli.quiet).await?;
                    }
                },
            }
        }
        Command::Export {
            ref export_format,
            ref output,
            no_local,
        } => {
            let config = resolve_config(&cli)?;
            let backend = create_backend(&config).await?;
            let env = secret::require_env(config.default_environment.as_deref())?;
            export::run(
                &backend,
                &config,
                env,
                export_format,
                output.as_deref(),
                no_local,
            )
            .await?;
        }
        Command::Run {
            no_local,
            ref command,
        } => {
            let config = resolve_config(&cli)?;
            let backend = create_backend(&config).await?;
            let env = secret::require_env(config.default_environment.as_deref())?;
            let exit_code = run::run(&backend, &config, env, no_local, command).await?;
            process::exit(exit_code);
        }
        Command::Import {
            ref file,
            ref import_format,
            overwrite,
            dry_run,
        } => {
            let config = resolve_config(&cli)?;
            let backend = create_backend(&config).await?;
            let env = secret::require_env(config.default_environment.as_deref())?;
            import::run(
                &backend,
                env,
                file,
                import_format.as_ref(),
                overwrite,
                dry_run,
            )
            .await?;
        }
    }

    Ok(())
}
