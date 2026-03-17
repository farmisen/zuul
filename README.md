# Zuul

> *"Are you the Keymaster?"* — Zuul, Ghostbusters (1984)

A CLI tool for managing secrets across multiple environments, backed by Google Cloud Secret Manager.

## Features

- **Multi-environment** — Manage secrets across `dev`, `staging`, `production`, and any custom environments
- **GCP Secret Manager** — Secrets are stored in Google Cloud with IAM-based access control
- **Export formats** — Output secrets as dotenv, direnv, JSON, YAML, or shell exports
- **Run with secrets** — Inject secrets into any subprocess via `zuul run`
- **Import** — Bulk-import from `.env`, JSON, or YAML files
- **Local overrides** — Override backend values locally via `.zuul.local.toml` (never leaves your machine)
- **Metadata** — Attach key-value metadata (owner, rotate-by, description) to secrets

## Quick Start

```bash
# Build from source
cargo install --path .

# Provision infrastructure (creates environments, IAM, etc.)
cd terraform
cp terraform.tfvars.example terraform.tfvars  # edit with your values
terraform init && terraform apply
cd ..

# Initialize the local project config
zuul init --project my-gcp-project-123

# Set up authentication
zuul auth

# Manage secrets (environments are already created by Terraform)
zuul secret set DATABASE_URL --env dev "postgres://localhost:5432/mydb"
zuul secret get DATABASE_URL --env dev

# Run with secrets injected
zuul run --env dev -- cargo run

# Export secrets
zuul export --env dev --export-format dotenv > .env
zuul export --env dev --export-format direnv > .envrc

# Import from an existing .env file
zuul import --env dev --file .env.local
```

## Configuration

### `.zuul.toml`

Created by `zuul init`. Committed to version control.

```toml
[backend]
type = "gcp-secret-manager"
project_id = "my-gcp-project-123"

[defaults]
environment = "dev"
```

### `.zuul.local.toml`

Local overrides for development. Added to `.gitignore` automatically.

```toml
[secrets]
DATABASE_URL = "postgres://localhost:5432/mydb_local"
REDIS_URL = "redis://localhost:6379"
```

Local overrides apply to `zuul export` and `zuul run` by default. Use `--no-local` to skip them.

## direnv Integration

Add this to your `.envrc` for automatic secret loading:

```bash
eval "$(zuul export --env dev --export-format direnv)"
```

## Infrastructure

A Terraform module is included in [`terraform/`](terraform/) to provision the GCP backend — it enables the Secret Manager API, creates the zuul environment registry, and sets up IAM bindings.

See [`terraform/README.md`](terraform/README.md) for details on IAM bindings, per-environment access scoping, and service account creation.

## Commands

| Command | Description |
|---------|-------------|
| `zuul init` | Initialize a new project |
| `zuul auth` | Set up GCP authentication |
| `zuul env list\|show\|copy\|clear` | View and manage environment secrets |
| `zuul secret list\|get\|set\|delete\|info\|copy` | Manage secrets |
| `zuul secret metadata list\|set\|delete` | Manage secret metadata |
| `zuul export` | Export secrets in various formats |
| `zuul run` | Run a command with secrets injected |
| `zuul import` | Bulk-import secrets from a file |

Use `zuul --help` or `zuul <command> --help` for details.

## Development

```bash
# Build
cargo build

# Lint and format
cargo clippy -- -D warnings
cargo fmt
```

### Running Tests

**Unit tests** run without any external dependencies:

```bash
cargo test
```

**Integration tests** run against a GCP Secret Manager emulator and cover all commands, options, and access control logic:

```bash
# Start the emulator
docker compose -f docker-compose.emulator.yml up -d

# Run the integration suite (80 tests)
cargo test --test integration -- --ignored

# Stop the emulator when done
docker compose -f docker-compose.emulator.yml down
```

The emulator state is in-memory — restart it for a clean slate between runs. Each test uses a unique project ID, so re-runs within the same emulator session are safe.

## Documentation

- [Software Requirements Specification](docs/zuul-spec.md)
- [Implementation Plan](docs/implementation-plan.md)
- [Terraform Module](terraform/README.md)
