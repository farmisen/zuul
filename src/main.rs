use std::collections::HashMap;
use std::process;

use clap::{CommandFactory, Parser};
use rustls::crypto::ring::default_provider;

use zuul::backend::file_backend::FileBackend;
use zuul::backend::gcp::GcpClient;
use zuul::backend::gcp_backend::GcpBackend;
use zuul::backend::{Backend, BackendKind};
use zuul::cli::{
    Cli, Command, DeployCommand, EnvCommand, MetadataCommand, RecoverCommand, SecretCommand,
    SyncCommand, audit, auth, diff, env, export, import, init, metadata, recover, run, secret,
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
    let config = load_config(
        &cwd,
        &CliOverrides {
            environment: env.map(String::from),
            project_id: cli.project.clone(),
            config_path: cli.config.clone(),
        },
    )?;
    config.require_config()?;
    Ok(config)
}

async fn create_backend(config: &Config) -> Result<BackendKind, ZuulError> {
    match config.backend_type.as_str() {
        "gcp-secret-manager" => {
            let project_id = config.project_id.as_deref().ok_or_else(|| {
                ZuulError::Config(
                    "No project ID configured. Run 'zuul init' to set up your project.".to_string(),
                )
            })?;
            let client = GcpClient::new(project_id, config.credentials.as_deref()).await?;
            Ok(BackendKind::Gcp(GcpBackend::new(
                client,
                config.credentials.clone(),
            )))
        }
        "file" => {
            let config_dir = config.config_dir.as_deref().ok_or_else(|| {
                ZuulError::Config(
                    "No .zuul.toml found. Run 'zuul init' to set up your project.".to_string(),
                )
            })?;
            let default_path = config_dir.join(".zuul.secrets.enc");
            let store_path = config
                .file_path
                .as_ref()
                .map(std::path::PathBuf::from)
                .unwrap_or(default_path);
            let identity = config.identity.as_ref().map(std::path::PathBuf::from);
            Ok(BackendKind::File(FileBackend::new(store_path, identity)))
        }
        "" => Err(ZuulError::Config(
            "No backend configured. Run 'zuul init --backend <type>' to set up your project. \
             Supported backends: gcp-secret-manager, file."
                .to_string(),
        )),
        other => Err(ZuulError::Config(format!(
            "Unknown backend type '{other}'. Supported: gcp-secret-manager, file."
        ))),
    }
}

async fn run(cli: Cli) -> Result<(), ZuulError> {
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
        Command::Auth { check, reconfigure } => {
            let config = resolve_config(&cli, None)?;
            auth::run(&config, check, reconfigure, cli.non_interactive).await?;
        }
        Command::Env { ref command } => {
            handle_env(&cli, command, progress).await?;
        }
        Command::Secret { ref command } => {
            handle_secret(&cli, command, progress).await?;
        }
        Command::Export {
            ref env,
            ref export_format,
            ref output,
            overrides,
        } => {
            handle_export(
                &cli,
                env.as_deref(),
                export_format,
                output.as_deref(),
                overrides,
                progress,
            )
            .await?;
        }
        Command::Run {
            ref env,
            overrides,
            ref command,
        } => {
            let exit_code = handle_run(&cli, env.as_deref(), overrides, command, progress).await?;
            process::exit(exit_code);
        }
        Command::Import {
            ref env,
            ref file,
            ref import_format,
            overwrite,
            dry_run,
        } => {
            handle_import(
                &cli,
                env.as_deref(),
                file,
                import_format.as_ref(),
                overwrite,
                dry_run,
                progress,
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
            handle_recover(&cli, command, progress).await?;
        }
        Command::Deploy { ref command } => {
            let exit_code = handle_deploy(&cli, command, progress).await?;
            process::exit(exit_code);
        }
        Command::Sync { ref command } => {
            handle_sync(&cli, command, progress).await?;
        }
        Command::Audit {
            ref env,
            ref identity,
        } => {
            let config = resolve_config(&cli, None)?;
            let backend = create_backend(&config).await?;
            audit::run(&backend, env.as_deref(), identity.as_deref(), &cli.format).await?;
        }
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "zuul", &mut std::io::stdout());
        }
    }

    Ok(())
}

async fn handle_env(
    cli: &Cli,
    command: &EnvCommand,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let config = resolve_config(cli, None)?;
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
        EnvCommand::Delete { name, force } => {
            let ctx = BatchContext {
                progress,
                project_root: config.config_dir.clone(),
            };
            env::delete(&backend, name, *force, &cli.format, &ctx).await?;
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
    Ok(())
}

async fn handle_secret(
    cli: &Cli,
    command: &SecretCommand,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    match command {
        SecretCommand::List { env, with_metadata } => {
            let config = resolve_config(cli, env.as_deref())?;
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
            let config = resolve_config(cli, env.as_deref())?;
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
            let config = resolve_config(cli, env.as_deref())?;
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
            let config = resolve_config(cli, env.as_deref())?;
            let backend = create_backend(&config).await?;
            let env = config.default_environment.as_deref();
            secret::delete(&backend, name, env, *force, *dry_run, &cli.format, progress).await?;
        }
        SecretCommand::Info { name, env } => {
            let config = resolve_config(cli, env.as_deref())?;
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
            let config = resolve_config(cli, None)?;
            let backend = create_backend(&config).await?;
            secret::copy(&backend, name, from, to, *force, progress).await?;
        }
        SecretCommand::Metadata { command: meta_cmd } => {
            handle_metadata(cli, meta_cmd, progress).await?;
        }
    }
    Ok(())
}

async fn handle_metadata(
    cli: &Cli,
    command: &MetadataCommand,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let config = resolve_config(cli, None)?;
    let backend = create_backend(&config).await?;
    match command {
        MetadataCommand::List { name, env } => {
            metadata::list(&backend, name, env.as_deref(), &cli.format).await?;
        }
        MetadataCommand::Set {
            name,
            key,
            value,
            env,
        } => {
            let ctx = BatchContext {
                progress,
                project_root: config.config_dir.clone(),
            };
            metadata::set(&backend, name, env.as_deref(), key, value, &ctx).await?;
        }
        MetadataCommand::Delete { name, key, env } => {
            let ctx = BatchContext {
                progress,
                project_root: config.config_dir.clone(),
            };
            metadata::delete(&backend, name, env.as_deref(), key, &ctx).await?;
        }
    }
    Ok(())
}

async fn handle_export(
    cli: &Cli,
    env_arg: Option<&str>,
    export_format: &zuul::cli::ExportFormat,
    output: Option<&std::path::Path>,
    overrides: bool,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let config = resolve_config(cli, env_arg)?;
    let backend = create_backend(&config).await?;
    let env = secret::require_env(config.default_environment.as_deref())?;
    export::run(
        &backend,
        &config,
        env,
        export_format,
        output,
        overrides,
        progress,
    )
    .await
}

async fn handle_run(
    cli: &Cli,
    env_arg: Option<&str>,
    overrides: bool,
    command: &[String],
    progress: ProgressOpts,
) -> Result<i32, ZuulError> {
    let config = resolve_config(cli, env_arg)?;
    let backend = create_backend(&config).await?;
    let env = secret::require_env(config.default_environment.as_deref())?;
    run::run(&backend, &config, env, overrides, command, progress).await
}

async fn handle_import(
    cli: &Cli,
    env_arg: Option<&str>,
    file: &std::path::Path,
    import_format: Option<&zuul::cli::ImportFormat>,
    overwrite: bool,
    dry_run: bool,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let config = resolve_config(cli, env_arg)?;
    let backend = create_backend(&config).await?;
    let env = secret::require_env(config.default_environment.as_deref())?;
    let ctx = BatchContext {
        progress,
        project_root: config.config_dir.clone(),
    };
    import::run(&backend, env, file, import_format, overwrite, dry_run, &ctx).await
}

async fn handle_recover(
    cli: &Cli,
    command: &RecoverCommand,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let config = resolve_config(cli, None)?;
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
    Ok(())
}

async fn handle_deploy(
    cli: &Cli,
    command: &DeployCommand,
    progress: ProgressOpts,
) -> Result<i32, ZuulError> {
    let config = resolve_config(cli, None)?;
    let backend = create_backend(&config).await?;
    match command {
        DeployCommand::Fly {
            env,
            app,
            no_sync,
            fly_args,
        } => {
            use zuul::cli::deploy;

            backend.get_environment(env).await?;
            let sp = zuul::progress::spinner("Fetching secrets...", progress);
            let backend_secrets = backend.list_secrets_for_environment(env).await?;
            sp.finish_and_clear();

            let secrets: HashMap<String, String> = backend_secrets
                .into_iter()
                .map(|(name, sv)| (name, sv.value))
                .collect();

            deploy::fly::run(secrets, app.as_deref(), *no_sync, fly_args, progress)
        }
    }
}

async fn handle_sync(
    cli: &Cli,
    command: &SyncCommand,
    progress: ProgressOpts,
) -> Result<(), ZuulError> {
    let config = resolve_config(cli, None)?;
    let backend = create_backend(&config).await?;
    match command {
        SyncCommand::Netlify {
            env,
            context,
            scope,
            dry_run,
            prune,
            force,
        } => {
            use zuul::cli::sync::{self, SyncTarget, netlify::NetlifyTarget};

            let target = NetlifyTarget::new(context, scope)?;

            backend.get_environment(env).await?;
            let sp = zuul::progress::spinner("Fetching secrets...", progress);
            let backend_secrets = backend.list_secrets_for_environment(env).await?;
            sp.finish_and_clear();

            let zuul_secrets: HashMap<String, String> = backend_secrets
                .into_iter()
                .map(|(name, sv)| (name, sv.value))
                .collect();

            let sp = zuul::progress::spinner(
                &format!("Fetching {} variables...", target.name()),
                progress,
            );
            let platform_vars = target.list_vars()?;
            sp.finish_and_clear();

            let actions = sync::compute_diff(&zuul_secrets, &platform_vars, *prune);
            sync::execute_sync(&sync::SyncOpts {
                target: &target,
                actions: &actions,
                dry_run: *dry_run,
                prune: *prune,
                force: *force,
                non_interactive: cli.non_interactive,
            })?;
        }
        SyncCommand::Fly {
            env,
            app,
            stage,
            dry_run,
            prune,
            force,
        } => {
            use zuul::cli::sync::{self, SyncTarget, fly::FlyTarget};

            let target = FlyTarget::new(app.as_deref(), *stage);

            backend.get_environment(env).await?;
            let sp = zuul::progress::spinner("Fetching secrets...", progress);
            let backend_secrets = backend.list_secrets_for_environment(env).await?;
            sp.finish_and_clear();

            let zuul_secrets: HashMap<String, String> = backend_secrets
                .into_iter()
                .map(|(name, sv)| (name, sv.value))
                .collect();

            let sp = zuul::progress::spinner(
                &format!("Fetching {} secrets...", target.name()),
                progress,
            );
            let platform_vars = target.list_vars()?;
            sp.finish_and_clear();

            let actions = sync::compute_diff(&zuul_secrets, &platform_vars, *prune);
            sync::execute_sync(&sync::SyncOpts {
                target: &target,
                actions: &actions,
                dry_run: *dry_run,
                prune: *prune,
                force: *force,
                non_interactive: cli.non_interactive,
            })?;

            if *stage && !*dry_run {
                println!(
                    "\nSecrets staged. Run `fly secrets deploy{}` to apply.",
                    app.as_ref()
                        .map(|a| format!(" --app {a}"))
                        .unwrap_or_default()
                );
            }
        }
    }
    Ok(())
}
