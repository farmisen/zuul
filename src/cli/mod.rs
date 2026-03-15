pub mod auth;
pub mod diff;
pub mod env;
pub mod export;
pub mod import;
pub mod init;
pub mod metadata;
pub mod run;
pub mod secret;

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// CLI tool for managing secrets across multiple environments.
#[derive(Debug, Parser)]
#[command(name = "zuul", version, about)]
pub struct Cli {
    /// Override GCP project ID
    #[arg(long, global = true)]
    pub project: Option<String>,

    /// Output format
    #[arg(long, global = true, default_value = "text")]
    pub format: OutputFormat,

    /// Path to config file
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Suppress non-essential output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Verbose output for debugging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Disable interactive prompts and progress indicators
    #[arg(long, global = true)]
    pub non_interactive: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// Output format for commands.
#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize a new zuul project in the current directory
    Init {
        /// GCP project ID
        #[arg(long)]
        project: Option<String>,

        /// Backend type
        #[arg(long, default_value = "gcp-secret-manager")]
        backend: String,
    },

    /// Verify and set up authentication with the backend
    Auth {
        /// Non-interactive validation only (exit code 0/1)
        #[arg(long)]
        check: bool,
    },

    /// Manage environments
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },

    /// Manage secrets
    Secret {
        #[command(subcommand)]
        command: SecretCommand,
    },

    /// Export all secrets for an environment
    Export {
        /// Target environment (overrides default from config)
        #[arg(short, long)]
        env: Option<String>,

        /// Export format
        #[arg(long = "export-format", value_enum)]
        export_format: ExportFormat,

        /// Write to file instead of stdout
        #[arg(long)]
        output: Option<PathBuf>,

        /// Skip local overrides
        #[arg(long)]
        no_local: bool,
    },

    /// Inject secrets into a subprocess as environment variables
    Run {
        /// Target environment (overrides default from config)
        #[arg(short, long)]
        env: Option<String>,

        /// Skip local overrides
        #[arg(long)]
        no_local: bool,

        /// Command and arguments to run
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
    },

    /// Bulk-import secrets from a file into an environment
    Import {
        /// Target environment (overrides default from config)
        #[arg(short, long)]
        env: Option<String>,

        /// Path to the file to import
        #[arg(long)]
        file: PathBuf,

        /// Import format (auto-detected from extension if omitted)
        #[arg(long = "import-format", value_enum)]
        import_format: Option<ImportFormat>,

        /// Overwrite existing secrets
        #[arg(long)]
        overwrite: bool,

        /// Preview what would be imported without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Compare secrets between two environments
    Diff {
        /// First environment
        env_a: String,

        /// Second environment
        env_b: String,

        /// Show actual secret values (masked by default)
        #[arg(long)]
        show_values: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum EnvCommand {
    /// List all environments
    List,

    /// Create a new environment
    Create {
        /// Environment name
        name: String,

        /// Optional description
        #[arg(long)]
        description: Option<String>,
    },

    /// Show environment details
    Show {
        /// Environment name
        name: String,
    },

    /// Update an environment
    Update {
        /// Environment name
        name: String,

        /// New name (rename)
        #[arg(long)]
        new_name: Option<String>,

        /// New description
        #[arg(long)]
        description: Option<String>,
    },

    /// Delete an environment
    Delete {
        /// Environment name
        name: String,

        /// Preview what would be deleted without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Copy all secrets from one environment to another
    Copy {
        /// Source environment
        from: String,

        /// Target environment
        to: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,

        /// Preview what would be copied without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Clear all secrets from an environment (keeps the environment itself)
    Clear {
        /// Environment name
        name: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,

        /// Preview what would be cleared without making changes
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum SecretCommand {
    /// List secrets
    List {
        /// Target environment (overrides default from config)
        #[arg(short, long)]
        env: Option<String>,
    },

    /// Get a secret's value
    Get {
        /// Secret name
        name: String,

        /// Target environment (overrides default from config)
        #[arg(short, long)]
        env: Option<String>,
    },

    /// Set a secret's value
    Set {
        /// Secret name
        name: String,

        /// Secret value (if not using --from-file or --from-stdin)
        value: Option<String>,

        /// Read value from file
        #[arg(long)]
        from_file: Option<PathBuf>,

        /// Read value from stdin
        #[arg(long)]
        from_stdin: bool,

        /// Target environment (overrides default from config)
        #[arg(short, long)]
        env: Option<String>,
    },

    /// Delete a secret
    Delete {
        /// Secret name
        name: String,

        /// Force delete without confirmation
        #[arg(long)]
        force: bool,

        /// Preview what would be deleted
        #[arg(long)]
        dry_run: bool,

        /// Target environment (overrides default from config)
        #[arg(short, long)]
        env: Option<String>,
    },

    /// Show secret info and metadata
    Info {
        /// Secret name
        name: String,

        /// Target environment (overrides default from config)
        #[arg(short, long)]
        env: Option<String>,
    },

    /// Copy a secret from one environment to another
    Copy {
        /// Secret name
        name: String,

        /// Source environment
        #[arg(long)]
        from: String,

        /// Target environment
        #[arg(long)]
        to: String,

        /// Overwrite if exists in target
        #[arg(long)]
        force: bool,
    },

    /// Manage secret metadata
    Metadata {
        #[command(subcommand)]
        command: MetadataCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum MetadataCommand {
    /// List all metadata for a secret
    List {
        /// Secret name
        name: String,

        /// Target environment (overrides default from config)
        #[arg(short, long)]
        env: Option<String>,
    },

    /// Set a metadata key-value pair
    Set {
        /// Secret name
        name: String,

        /// Metadata key
        key: String,

        /// Metadata value
        value: String,

        /// Target environment (overrides default from config)
        #[arg(short, long)]
        env: Option<String>,
    },

    /// Delete a metadata key
    Delete {
        /// Secret name
        name: String,

        /// Metadata key
        key: String,

        /// Target environment (overrides default from config)
        #[arg(short, long)]
        env: Option<String>,
    },
}

/// Export format options.
#[derive(Debug, Clone, ValueEnum)]
pub enum ExportFormat {
    Dotenv,
    Direnv,
    Json,
    Yaml,
    Shell,
}

/// Import format options.
#[derive(Debug, Clone, ValueEnum)]
pub enum ImportFormat {
    Dotenv,
    Json,
    Yaml,
}
