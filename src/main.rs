use clap::Parser;

use zuul::cli::{Cli, Command, EnvCommand, SecretCommand};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { .. } => todo!("zuul init"),
        Command::Auth { .. } => todo!("zuul auth"),
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
}
