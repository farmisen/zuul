use std::process;

use clap::Parser;

use zuul::backend::gcp::GcpClient;
use zuul::backend::gcp_backend::GcpBackend;
use zuul::cli::{Cli, Command, EnvCommand, SecretCommand, auth, env, init};
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
        Command::Secret { command } => match command {
            SecretCommand::List => todo!("zuul secret list"),
            SecretCommand::Get { .. } => todo!("zuul secret get"),
            SecretCommand::Set { .. } => todo!("zuul secret set"),
            SecretCommand::Delete { .. } => todo!("zuul secret delete"),
            SecretCommand::Info { .. } => todo!("zuul secret info"),
            SecretCommand::Copy { .. } => todo!("zuul secret copy"),
            SecretCommand::Metadata { .. } => todo!("zuul secret metadata"),
        },
        Command::Export { .. } => todo!("zuul export"),
        Command::Run { .. } => todo!("zuul run"),
        Command::Import { .. } => todo!("zuul import"),
    }

    Ok(())
}
