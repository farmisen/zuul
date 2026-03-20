# GCP Secret Manager Backend

The GCP backend stores secrets in Google Cloud Secret Manager with IAM-based access control. Designed for teams, CI/CD pipelines, and production deployments.

## Setup

```bash
# 1. Provision infrastructure
cd terraform
cp terraform.tfvars.example terraform.tfvars  # edit with your values
terraform init && terraform apply
cd ..

# 2. Initialize the project
zuul init --project my-gcp-project-123

# 3. Authenticate
zuul auth
```

## Configuration

```toml
[backend]
type = "gcp-secret-manager"
project_id = "my-gcp-project-123"

# Optional: path to service account key (overridden by ZUUL_GCP_CREDENTIALS env var)
# credentials = "/path/to/service-account.json"

[defaults]
environment = "dev"
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `ZUUL_GCP_PROJECT` | Override GCP project ID (takes precedence over `.zuul.toml`) |
| `ZUUL_GCP_CREDENTIALS` | GCP service account key: file path or inline JSON (see below) |

## Naming Convention

Each zuul-managed secret maps to one GCP Secret Manager secret:

```
zuul__{environment}__{secret_name}
```

| Zuul secret | Environment | GCP secret name |
|---|---|---|
| `DATABASE_URL` | `production` | `zuul__production__DATABASE_URL` |
| `DATABASE_URL` | `dev` | `zuul__dev__DATABASE_URL` |

## Authentication

Three modes:

1. **Application Default Credentials (ADC)** — `gcloud auth application-default login`. Default for local development.
2. **Service Account Key File** — via `ZUUL_GCP_CREDENTIALS` env var or config `credentials` field. For CI/CD.
3. **Inline JSON** — pass the service account key JSON directly via `ZUUL_GCP_CREDENTIALS`. Zuul auto-detects whether the value is a file path or inline JSON (starts with `{` → JSON, otherwise → file path). The JSON is written to a secure temporary file (mode 0600) at runtime and deleted on process exit. Useful for CI/CD platforms that inject secrets as environment variables rather than files.

```bash
# File path (existing behavior)
export ZUUL_GCP_CREDENTIALS="/path/to/service-account.json"

# Inline JSON (new)
export ZUUL_GCP_CREDENTIALS='{"type":"service_account","project_id":"my-project",...}'
```

## Permissions Model

Zuul delegates all access control to GCP IAM. No client-side permission logic.

| Role | GCP IAM | Can do |
|------|---------|--------|
| **Admin** | `secretmanager.admin` (full project) | Manage environments (via Terraform), read/write all secrets |
| **Developer** | `secretmanager.secretAccessor` (scoped to `zuul__dev__*`) | Read/write secrets in their scoped environment |
| **CI/CD** | `secretmanager.secretAccessor` (scoped to target env) | Read secrets for deployment |

## Environment Management

Environments are managed by **Terraform**, not the CLI. This ensures the registry and IAM bindings are always in sync.

```bash
# CLI commands return an error for GCP:
zuul env create dev
# Error: Environment management is handled by Terraform for the GCP backend.
#        Run `terraform apply` to create environments.
```

See the [Environment Admin Playbook](gcp-env-playbook.md) for step-by-step procedures:
- Creating a new environment
- Renaming an environment
- Decommissioning an environment
- Rotating IAM bindings
- Recovering from drift

## Terraform Module

The [`terraform/`](../terraform/) directory contains a ready-to-use module that:

- Enables the Secret Manager API
- Creates the zuul environment registry (`zuul__registry`)
- Sets up IAM bindings with per-environment access scoping
- Optionally creates CI/CD service accounts

See [`terraform/README.md`](../terraform/README.md) for details.

## Labels and Annotations

Each GCP secret is tagged with labels for efficient filtering:

| Label key | Value | Purpose |
|---|---|---|
| `zuul-managed` | `true` | Identify zuul-managed secrets |
| `zuul-env` | environment name | Filter by environment |
| `zuul-name` | secret name | Group same logical secret across environments |

User-defined metadata is stored as GCP annotations with the `zuul-meta--` prefix.
