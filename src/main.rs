use std::process;

use clap::Parser;

use zuul::cli::{Cli, Command, EnvCommand, SecretCommand, auth, init};
use zuul::config::{CliOverrides, load_config};
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

async fn run(cli: Cli) -> Result<(), ZuulError> {
    match cli.command {
        Command::Init { project, backend } => {
            let cwd = get_cwd()?;
            init::run(&cwd, project, &backend)?;
        }
        Command::Auth { check } => {
            let cwd = get_cwd()?;
            let config = load_config(
                &cwd,
                &CliOverrides {
                    project_id: cli.project.clone(),
                    config_path: cli.config.clone(),
                    ..Default::default()
                },
            )?;
            auth::run(&config, check).await?;
        }
        Command::Env { command } => match command {
            EnvCommand::List => todo!("zuul env list"),
            EnvCommand::Create { .. } => todo!("zuul env create"),
            EnvCommand::Show { .. } => todo!("zuul env show"),
            EnvCommand::Update { .. } => todo!("zuul env update"),
            EnvCommand::Delete { .. } => todo!("zuul env delete"),
        },
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
