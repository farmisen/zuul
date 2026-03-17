# Zuul

> *"Are you the Keymaster?"* — Zuul, Ghostbusters (1984)

A CLI tool for managing secrets across multiple environments, backed by Google Cloud Secret Manager.

## Features

- **Multi-environment** — Manage secrets across `dev`, `staging`, `production`, and any custom environments
- **GCP Secret Manager** — Secrets are stored in Google Cloud with IAM-based access control
- **File backend** — Encrypted local storage via `age` for offline use, small projects, and local development
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

**Caveats:**
- Requires an active GCP auth session (`zuul auth` or `gcloud auth application-default login`)
- Adds latency on each `cd` into the project (one API call to fetch secrets)
- The `.envrc` file itself contains no secrets — safe to commit to version control

See [`.envrc.example`](.envrc.example) for a ready-to-use template.

## File Backend

For local development or small projects that don't need cloud infrastructure:

```bash
zuul init --backend file
# Enter a passphrase when prompted (or set ZUUL_PASSPHRASE env var)

zuul env create dev
zuul secret set DATABASE_URL --env dev "postgres://localhost/mydb"
zuul run --env dev -- cargo run
```

All secrets are stored in a single encrypted file (`.zuul.secrets.enc`) using `age` passphrase encryption. The file is automatically added to `.gitignore`.

## Infrastructure (GCP)

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
| `zuul diff` | Compare secrets between two environments |
| `zuul recover status\|resume\|abort` | Inspect or resume incomplete batch operations |
| `zuul completions <shell>` | Generate shell completions (bash, zsh, fish, etc.) |

Use `zuul --help` or `zuul <command> --help` for details.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `ZUUL_GCP_PROJECT` | Override GCP project ID (takes precedence over `.zuul.toml`) |
| `ZUUL_GCP_CREDENTIALS` | Path to GCP service account key file |
| `ZUUL_DEFAULT_ENV` | Override default environment name |
| `ZUUL_BACKEND` | Override backend type |

**Resolution order** (highest priority first): CLI flags → environment variables → `.zuul.local.toml` (secrets only) → `.zuul.toml` → built-in defaults.

## Permissions Model

Zuul delegates all access control to GCP IAM. No client-side permission logic.

| Role | GCP IAM | Can do |
|------|---------|--------|
| **Admin** | `secretmanager.admin` (full project) | Manage environments (via Terraform), read/write all secrets |
| **Developer** | `secretmanager.secretAccessor` (scoped to `zuul__dev__*`) | Read/write secrets in their scoped environment |
| **CI/CD** | `secretmanager.secretAccessor` (scoped to target env) | Read secrets for deployment |

Environments are managed by Terraform, which creates both the registry entries and IAM bindings in a single `terraform apply`. See the [Environment Admin Playbook](docs/env-admin-playbook.md) for operational procedures.

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
- [Environment Admin Playbook](docs/env-admin-playbook.md)
