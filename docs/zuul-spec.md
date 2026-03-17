# Zuul — Software Requirements Specification

> *"Are you the Keymaster?"* — Zuul, Ghostbusters (1984)

**Version:** 0.1.0 (MVP)
**Status:** Draft / Research Phase

---

## 1. Overview

Zuul is a command-line tool for managing secrets (environment values of any sensitivity) across multiple environments. It provides a unified interface for storing, retrieving, and exporting secrets backed by pluggable secret management services.

**Key principles:**

- Backend-agnostic design with Google Cloud Secret Manager as the MVP backend
- All access control delegated to the backend (no client-side permission logic)
- Simple mental model: a secret has a name, a value per environment, and optional metadata
- Export secrets in formats consumable by different toolchains (direnv, CI/CD, Docker, etc.)

---

## 2. Glossary

| Term | Definition |
|---|---|
| **Secret** | A named environment value managed by zuul. May or may not be sensitive — zuul treats all managed values uniformly. |
| **Environment** | A named deployment context (e.g., `production`, `staging`, `dev`). Secrets are scoped per environment. |
| **Backend** | The storage service that persists secret values and metadata. MVP: Google Cloud Secret Manager. |
| **Project** | A zuul project corresponds to a single backend configuration (e.g., one GCP project). |
| **Registry** | A backend-stored record of known environments and project-level metadata. |

---

## 3. Architecture

### 3.1 Backend Abstraction

Zuul defines a `Backend` trait that all storage implementations must satisfy. This allows swapping backends without changing the CLI logic.

```
trait Backend {
    // Environment operations
    fn list_environments() -> Result<Vec<Environment>>
    fn create_environment(name, metadata?) -> Result<Environment>
    fn get_environment(name) -> Result<Environment>
    fn update_environment(name, metadata) -> Result<Environment>
    fn delete_environment(name) -> Result<()>

    // Secret operations
    fn list_secrets(filter?) -> Result<Vec<SecretEntry>>
    fn get_secret(name, environment) -> Result<SecretValue>
    fn set_secret(name, environment, value) -> Result<()>
    fn delete_secret(name, environment) -> Result<()>

    // Metadata operations
    fn get_metadata(name, environment) -> Result<Metadata>
    fn set_metadata(name, environment, key, value) -> Result<()>
    fn delete_metadata(name, environment, key) -> Result<()>

    // Bulk operations
    fn list_secrets_for_environment(environment) -> Result<Vec<(String, SecretValue)>>
}
```

### 3.2 GCP Secret Manager Backend (MVP)

#### Naming Convention

Each zuul-managed secret maps to one GCP Secret Manager secret using the pattern:

```
zuul__{environment}__{secret_name}
```

The double-underscore delimiter was chosen because GCP secret names only allow `[a-zA-Z0-9_-]`, and double underscores are unlikely to appear in user-chosen names. Secret names containing `__` should be rejected by zuul with a clear error.

**Examples:**

| Zuul secret | Environment | GCP secret name |
|---|---|---|
| `DATABASE_URL` | `production` | `zuul__production__DATABASE_URL` |
| `DATABASE_URL` | `dev` | `zuul__dev__DATABASE_URL` |
| `STRIPE_KEY` | `production` | `zuul__production__STRIPE_KEY` |

#### Labels and Annotations

Each GCP secret is tagged with labels for efficient filtering:

| Label key | Value | Purpose |
|---|---|---|
| `zuul-managed` | `true` | Identify zuul-managed secrets |
| `zuul-env` | environment name | Filter by environment |
| `zuul-name` | secret name | Group same logical secret across environments |

User-defined metadata is stored as GCP annotations (which support values up to 1024 characters):

| Annotation key | Example value |
|---|---|
| `zuul-meta--description` | `Primary database connection string` |
| `zuul-meta--owner` | `backend-team` |
| `zuul-meta--rotate-by` | `2026-06-01` |
| `zuul-meta--source` | `AWS RDS console` |

#### Environment Registry

The list of known environments is stored in a dedicated GCP secret named `zuul__registry`. Its value is a JSON document:

```json
{
  "version": 1,
  "environments": {
    "production": {
      "description": "Live production environment",
      "created_at": "2026-03-10T12:00:00Z",
      "updated_at": "2026-03-10T12:00:00Z"
    },
    "staging": {
      "description": "Pre-production staging",
      "created_at": "2026-03-10T12:00:00Z",
      "updated_at": "2026-03-10T12:00:00Z"
    },
    "dev": {
      "description": "Local development",
      "created_at": "2026-03-10T12:00:00Z",
      "updated_at": "2026-03-10T12:00:00Z"
    }
  }
}
```

**Concurrency:** Environment CRUD is infrequent. For the MVP, last-write-wins is acceptable. The `version` field and GCP's native etag support provide a path to optimistic locking if needed later.

**Resilience:** If the registry is lost or corrupted, the environment list can be reconstructed by scanning labels on existing secrets (`zuul-managed=true`, aggregate distinct `zuul-env` values).

#### Authentication

The GCP backend supports two authentication modes:

1. **Application Default Credentials (ADC):** Uses the credentials from `gcloud auth application-default login`. This is the default for local development.
2. **Service Account Key File:** Specified via the config file or `ZUUL_GCP_CREDENTIALS` environment variable. Intended for CI/CD pipelines.

#### Permissions Model

Zuul does **not** implement its own authorization logic. All access control is delegated to GCP IAM. Recommended IAM roles:

| Persona | GCP Role | Scope | Can manage environments? |
|---|---|---|---|
| Developer | `roles/secretmanager.secretAccessor` | Scoped to `dev` secrets via IAM conditions on resource name (`zuul__dev__*`) | No |
| CI/CD pipeline | `roles/secretmanager.secretAccessor` | Scoped to target environment | No |
| Ops / Admin | `roles/secretmanager.admin` | Full project | Yes |

**Environment management is admin-only.** Creating, updating, and deleting environments requires write access to the `zuul__registry` secret, which should only be granted to the admin role. Developers and CI/CD pipelines only need read access to secrets within their scoped environments. The Terraform configuration (Section 8) enforces this by granting `secretAccessor` (read-only) to non-admin personas and restricting `secretmanager.admin` to ops.

This is the primary reason for choosing the one-secret-per-name+environment mapping — it lets GCP IAM conditions operate on the resource name directly without zuul needing to interpret or enforce permissions.

---

## 4. Data Model

### 4.1 Environment

```
Environment {
    name: String          // e.g., "production", "staging", "dev"
    description: String?  // optional human-readable description
    created_at: DateTime
    updated_at: DateTime
}
```

**Constraints:**

- Name must match `[a-z0-9][a-z0-9-]*` (lowercase, alphanumeric, hyphens allowed, no leading hyphen)
- Name must not contain `__`
- Name max length: 50 characters
- Names `registry` and `config` are reserved

### 4.2 Secret

```
SecretEntry {
    name: String              // e.g., "DATABASE_URL"
    environments: Vec<String> // environments where this secret exists
    metadata: Map<String, String>
}

SecretValue {
    name: String
    environment: String
    value: String             // the actual secret value (may be multi-line)
    version: String           // backend version identifier
    created_at: DateTime
    updated_at: DateTime
}
```

**Constraints:**

- Name must match `[A-Za-z_][A-Za-z0-9_-]*` (conventional env var naming)
- Name must not contain `__`
- Name max length: 200 characters
- Value max size: 64 KiB (GCP Secret Manager limit)

### 4.3 Metadata

Arbitrary key-value pairs attached to a logical secret, shared across all environments where it exists. Stored as GCP annotations with the `zuul-meta--` prefix on each underlying GCP secret.

**Shared behavior:** When metadata is set or deleted, the operation is applied to all environments where the secret exists. This keeps metadata consistent across environments (e.g., `owner` or `rotate-by` applies to `DATABASE_URL` regardless of environment). The `--env` flag can optionally scope the operation to a single environment.

**Reserved metadata keys** (zuul may use these for built-in features in the future):

- `description` — human-readable description of the secret
- `owner` — team or individual responsible
- `rotate-by` — date by which the secret should be rotated
- `source` — where the secret value originates

---

## 5. Configuration

### 5.1 Config File: `.zuul.toml`

Located at the project root (or a parent directory). Created by `zuul init`.

```toml
[backend]
type = "gcp-secret-manager"
project_id = "my-gcp-project-123"

# Optional: path to service account key (overridden by ZUUL_GCP_CREDENTIALS env var)
# credentials = "/path/to/service-account.json"

[defaults]
environment = "dev"

# Custom export format templates (in addition to built-in formats)
[export.formats.custom-k8s]
template = "{{key}}: {{value | base64}}"
separator = "\n"
```

### 5.2 Local Overrides: `.zuul.local.toml`

A developer can create a `.zuul.local.toml` file alongside `.zuul.toml` to override secret values locally without touching the backend. This file is purely local, should be added to `.gitignore`, and is never read by CI/CD pipelines.

```toml
# .zuul.local.toml — personal dev overrides (gitignored)

[secrets]
DATABASE_URL = "postgres://localhost:5432/mydb_local"
REDIS_URL = "redis://localhost:6379"
```

**Behavior:**

- When `zuul export` or `zuul run` resolves secrets for an environment, local overrides take precedence over backend values
- Overrides apply regardless of which `--env` is targeted (they are environment-agnostic since they represent the developer's local machine)
- `zuul secret list` and `zuul secret info` indicate when a secret has a local override active (e.g., a `(local)` marker)
- `zuul export --no-local` and `zuul run --no-local` skip local overrides (useful for testing against the real backend values)

**Rationale:** Developers often need to point services at local instances (local database, local Redis, mock APIs) while the team-wide `dev` environment targets shared resources. Local overrides avoid polluting the backend with per-developer environments and keep the source of truth clean.

`zuul init` adds `.zuul.local.toml` to `.gitignore` automatically.

### 5.3 Config Resolution Order

1. CLI flags (highest priority)
2. Environment variables (`ZUUL_BACKEND`, `ZUUL_GCP_PROJECT`, `ZUUL_GCP_CREDENTIALS`, `ZUUL_DEFAULT_ENV`)
3. `.zuul.local.toml` secret overrides (for `export` and `run` only)
4. `.zuul.toml` in current or ancestor directory
5. Built-in defaults (lowest priority)

---

## 6. CLI Interface

### 6.1 Global Flags

```
--env, -e <ENV>         Target environment (overrides default from config)
--project <PROJECT_ID>  Override GCP project ID
--format <FORMAT>       Output format: text (default), json
--config <PATH>         Path to config file
--quiet, -q             Suppress non-essential output
--verbose, -v           Verbose output for debugging
```

### 6.2 Commands

#### `zuul init`

Initialize a new zuul project in the current directory.

```
zuul init [--project <GCP_PROJECT_ID>] [--backend <BACKEND_TYPE>]
```

Creates a `.zuul.toml` file. If `--project` is not supplied, prompts interactively.

#### `zuul auth`

Verify and set up authentication with the backend.

```
zuul auth [--check]
```

**Without flags:** Walks the developer through authentication setup:

1. Checks for existing Application Default Credentials
2. If missing, prompts to run `gcloud auth application-default login` (and offers to run it)
3. Validates that the credentials can reach the configured GCP project
4. Tests that the user can list secrets (i.e., has at least `secretAccessor` on some scope)
5. Reports the authenticated identity and accessible environments

```
$ zuul auth
Checking credentials for GCP project 'my-gcp-project-123'...
Authenticated as: developer@company.com
Accessible environments: dev
Ready to go! Try: zuul secret list --env dev
```

**`--check`:** Non-interactive validation only. Returns exit code 0 if credentials are valid, 1 otherwise. Useful in CI/CD scripts.

**Auto-detection:** Any zuul command that fails due to missing or expired credentials prints:

```
Error: No valid credentials found. Run 'zuul auth' to set up authentication.
```

#### `zuul env`

View environments and manage their secrets.

```
zuul env list                                          # List all environments
zuul env show <name>                                   # Show environment details + secret count
zuul env copy <from> <to> [--force] [--dry-run]        # Copy all secrets between environments
zuul env clear <name> [--force] [--dry-run]            # Delete all secrets (keeps environment)
```

**Environment lifecycle (create, update, delete) is managed by Terraform**, not the CLI. Environments are infrastructure — they define IAM security boundaries and must be managed alongside their permission bindings. See [`terraform/`](../terraform/) and the [Environment Admin Playbook](env-admin-playbook.md) for details.

`env clear --force` can be used as a Terraform `local-exec` pre-destroy provisioner to remove all bound secrets before `terraform destroy` removes the environment from the registry.

All batch operations (`clear`, `copy`, `import`, `metadata set/delete`) use a journal file (`.zuul/journal.json`) for crash recovery. If interrupted, `zuul recover` can resume or abort the operation.

#### `zuul secret`

Manage individual secrets.

```
zuul secret list [--env <e>]                       # List secrets; optionally filter by env
zuul secret get <name> --env <e>                   # Print secret value to stdout
zuul secret set <name> --env <e> [VALUE]           # Set a secret value
zuul secret set <name> --env <e> --from-file <f>   # Set value from file contents
zuul secret set <name> --env <e> --from-stdin      # Set value from stdin
zuul secret delete <name> --env <e> [--force] [--dry-run]  # Delete a secret
zuul secret info <name> [--env <e>]                # Show metadata and which envs it exists in
```

**`zuul secret set` value resolution order:**

1. `--from-file <path>` — read value from file
2. `--from-stdin` — read value from stdin
3. Positional `VALUE` argument
4. If none of the above: prompt interactively (with hidden input)

**`zuul secret list` output:**

Without `--env`:
```
NAME              ENVIRONMENTS
DATABASE_URL      dev, staging, production
STRIPE_KEY        staging, production
DEBUG_MODE        dev
```

With `--env production`:
```
NAME              UPDATED
DATABASE_URL      2026-03-08 14:22
STRIPE_KEY        2026-03-10 09:15
```

#### `zuul secret copy`

Copy a secret's value from one environment to another.

```
zuul secret copy <name> --from <env> --to <env>
```

This is a convenience wrapper around `get` + `set`. If the secret already exists in the target environment, zuul prompts for confirmation (or use `--force` to overwrite without prompting). Metadata is **not** copied — only the value.

#### `zuul secret metadata`

Manage secret metadata. Metadata is shared across all environments where a secret exists. The `--env` flag is optional — if omitted, the operation applies to all environments.

```
zuul secret metadata list <name> [--env <e>]                # List all metadata k/v pairs
zuul secret metadata set <name> <key> <value> [--env <e>]   # Set a metadata entry (all envs)
zuul secret metadata delete <name> <key> [--env <e>]        # Remove a metadata entry (all envs)
```

#### `zuul export`

Export all secrets for an environment.

```
zuul export --env <e> --format <fmt> [--output <file>] [--no-local]
```

**Built-in formats:**

| Format | Flag | Output |
|---|---|---|
| dotenv | `--format dotenv` | `KEY=value` (with proper escaping/quoting) |
| direnv | `--format direnv` | `export KEY='value'` |
| json | `--format json` | `{"KEY": "value", ...}` |
| yaml | `--format yaml` | `KEY: value` |
| shell | `--format shell` | `export KEY='value'` (identical to direnv for now, distinct for future divergence) |

If `--output` is omitted, writes to stdout. This allows piping:

```bash
zuul export --env dev --format direnv > .envrc
```

#### `zuul run`

Inject secrets into a subprocess as environment variables.

```
zuul run --env <e> [--no-local] -- <command> [args...]
```

Fetches all secrets for the given environment, injects them into the child process's environment (merged with the current environment), and executes the command. The parent shell's environment is **not** modified.

```bash
# Example: run the app with production secrets
zuul run --env production -- node server.js

# Example: run a one-off migration
zuul run --env staging -- python manage.py migrate
```

**Behavior:**

- Zuul's own env vars (e.g., `ZUUL_DEFAULT_ENV`) are **not** passed to the child
- If a secret name collides with an existing env var, the secret wins (with a warning on stderr)
- Exit code is forwarded from the child process

#### `zuul import`

Bulk-import secrets from a file into an environment.

```
zuul import --env <e> --file <path> [--format <fmt>] [--overwrite] [--dry-run]
```

**Supported input formats:** `dotenv` (default, auto-detected), `json`, `yaml`. The format is inferred from the file extension if `--format` is not specified.

**Behavior:**

- Parses the input file and creates/updates each key-value pair as a secret in the target environment
- By default, skips secrets that already exist in the target environment (with a warning). Use `--overwrite` to replace existing values.
- `--dry-run` lists the secrets that would be created/updated without making changes
- Reports a summary: `Imported 12 secrets (3 skipped, 2 overwritten) into environment 'dev'.`

```bash
# Import from an existing .env file
zuul import --env dev --file .env

# Import with overwrite, preview first
zuul import --env staging --file config.json --dry-run
zuul import --env staging --file config.json --overwrite
```

#### `zuul diff` (nice-to-have for MVP)

Compare secrets across environments.

```
zuul diff <env_a> <env_b>
```

```
NAME              dev          staging
DATABASE_URL      localhost    staging-db.internal
STRIPE_KEY        (not set)    sk_test_...
DEBUG_MODE        true         (not set)
```

#### `zuul recover`

Inspect or resume an incomplete batch operation. All batch operations (`import`, `env clear`, `env copy`, `metadata set/delete`) write a journal file (`.zuul/journal.json`) before starting. If a batch operation is interrupted, the journal records which steps completed and which are still pending.

```
zuul recover status                # Show pending operation details
zuul recover resume [--force]      # Resume from the first pending step
zuul recover abort [--force]       # Discard the journal (acknowledge partial state)
```

**`status`:** Read-only. Displays the operation type, parameters, start time, progress (e.g., "2 of 5 steps completed"), and lists the pending steps.

**`resume`:** Re-executes pending steps from where the operation left off. For `import`, the source file must still be accessible. Requires confirmation unless `--force` is passed.

**`abort`:** Deletes the journal file and prints a summary of the partial state left behind. The user is responsible for any manual cleanup. For deleted secrets, GCP Secret Manager's configurable destruction grace period allows recovery via `gcloud`. Requires confirmation unless `--force` is passed.

If no journal exists, all subcommands print "No incomplete operations found." and exit successfully.

---

## 7. Export Format Details

### 7.1 dotenv

Follows the [dotenv convention](https://www.dotenv.org/docs/security/env):

```
# Generated by zuul — environment: dev
# Do not edit manually. Re-export with: zuul export --env dev --format dotenv

DATABASE_URL="postgres://localhost:5432/mydb"
STRIPE_KEY="sk_test_abc123"
MULTILINE_CERT="-----BEGIN CERTIFICATE-----\nMIIBxTCCA...\n-----END CERTIFICATE-----"
```

Multi-line values are escaped with `\n` within double quotes.

### 7.2 direnv

```bash
# Generated by zuul — environment: dev
# Do not edit manually. Re-export with: zuul export --env dev --format direnv

export DATABASE_URL='postgres://localhost:5432/mydb'
export STRIPE_KEY='sk_test_abc123'
export MULTILINE_CERT='-----BEGIN CERTIFICATE-----
MIIBxTCCA...
-----END CERTIFICATE-----'
```

Single quotes preserve literal values including newlines.

### 7.3 direnv Integration

Zuul provides a recommended `.envrc` pattern for automatic secret loading via direnv:

```bash
# .envrc — automatically loads zuul secrets for the dev environment
eval "$(zuul export --env dev --format direnv)"
```

When a developer `cd`s into the project directory, direnv evaluates this, which fetches the latest secrets from the backend and exports them into the shell. This replaces the need to manually run `zuul export` or maintain a local `.env` file.

**Caveats to document:**

- Requires an active GCP auth session (`zuul auth` or `gcloud auth application-default login`)
- Adds latency on each `cd` (one API call per secret, or a batched list call). A future optimization could add local caching with a TTL.
- The `.envrc` file itself contains no secrets — it's safe to commit to version control

---

## 8. Terraform Configuration

Zuul requires a GCP project with the Secret Manager API enabled. The Terraform module should provision:

1. **GCP project** (or use an existing one)
2. **Secret Manager API** enabled
3. **IAM bindings** for standard personas:
   - Admin role for ops
   - Per-environment accessor roles using IAM conditions on secret name prefix (`zuul__{env}__*`)
4. **Service accounts** for CI/CD pipelines (one per environment or a shared one with scoped access)

```hcl
# Sketch — the actual Terraform module would be in terraform/

variable "project_id" {}
variable "environments" {
  type    = list(string)
  default = ["production", "staging", "dev"]
}
variable "ci_cd_service_accounts" {
  type    = map(string)  # environment -> SA email
  default = {}
}

resource "google_project_service" "secret_manager" {
  project = var.project_id
  service = "secretmanager.googleapis.com"
}

# Per-environment IAM condition example
resource "google_project_iam_member" "env_accessor" {
  for_each = toset(var.environments)

  project = var.project_id
  role    = "roles/secretmanager.secretAccessor"
  member  = "serviceAccount:${var.ci_cd_service_accounts[each.key]}"

  condition {
    title      = "zuul-${each.key}-secrets"
    expression = "resource.name.startsWith(\"projects/${var.project_id}/secrets/zuul__${each.key}__\")"
  }
}
```

Deliverable: a `terraform/` directory in the zuul repo with a ready-to-use module.

---

## 9. Error Handling

Zuul should provide clear, actionable error messages. Every error should suggest what the user can do next.

| Scenario | Message pattern |
|---|---|
| Secret not found | `Secret 'DB_URL' not found in environment 'production'. Run 'zuul secret list --env production' to see available secrets.` |
| Environment not found | `Environment 'qa' does not exist. Run 'zuul env list' to see available environments.` |
| Permission denied | `Permission denied for secret 'zuul__production__DB_URL'. Ensure your GCP identity has secretmanager.versions.access on this resource.` |
| No config file | `No .zuul.toml found. Run 'zuul init' to create one.` |
| Name validation | `Secret name 'my__secret' is invalid: names cannot contain '__' (reserved as delimiter).` |

---

## 10. Security Considerations

1. **Secret values are never written to disk** unless explicitly exported with `zuul export --output`. The `zuul run` command injects via process environment only.
2. **No client-side caching** of secret values in the MVP. Every read fetches from the backend.
3. **Stdout masking:** `zuul secret get` writes the raw value to stdout for piping, but `zuul secret list` and `zuul secret info` never display values.
4. **Audit trail:** GCP Secret Manager provides Cloud Audit Logs for all access — zuul does not need its own audit mechanism.
5. **`.zuul.toml` should not contain secret values.** Only project configuration.
6. **`.zuul.local.toml` contains secret values** and must be gitignored. `zuul init` enforces this automatically. This file never leaves the developer's machine.

---

## 11. Non-Goals (MVP)

These are explicitly out of scope for the initial release:

- Secret rotation automation (metadata supports tracking rotation dates; automation comes later)
- Secret value encryption at the client level (GCP handles encryption at rest)
- Multi-backend in a single project (one backend per `.zuul.toml`)
- Web UI or dashboard
- Secret versioning/rollback via zuul (GCP versions exist but are not exposed in MVP CLI)
- Secret sharing/syncing across GCP projects
- Built-in secret generation (e.g., random password generation)

---

## 12. Implementation Notes

### Language & Tooling

- **Language:** Rust
- **CLI framework:** `clap` (derive API)
- **GCP SDK:** `google-cloud-secretmanager` crate, or raw REST via `reqwest` + `google-authz` if the crate is lacking
- **Config parsing:** `toml` crate
- **Serialization:** `serde` + `serde_json`
- **Error handling:** `anyhow` for application errors, `thiserror` for library errors

### Project Structure (suggested)

```
zuul/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point, CLI parsing
│   ├── cli/                  # Command handlers
│   │   ├── mod.rs
│   │   ├── auth.rs
│   │   ├── env.rs
│   │   ├── secret.rs
│   │   ├── export.rs
│   │   ├── import.rs
│   │   └── run.rs
│   ├── backend/              # Backend trait + implementations
│   │   ├── mod.rs            # Backend trait definition
│   │   └── gcp.rs            # GCP Secret Manager implementation
│   ├── config.rs             # .zuul.toml parsing and resolution
│   ├── models.rs             # Data types (Environment, Secret, Metadata)
│   └── error.rs              # Error types
├── terraform/
│   ├── main.tf
│   ├── variables.tf
│   └── outputs.tf
├── tests/
│   ├── integration/          # Tests against a real or emulated GCP backend
│   └── unit/
└── README.md
```

---

## 13. Resolved Design Decisions

1. **Secret copying across environments:** Yes — MVP includes `zuul secret copy`.
2. **Wildcard exports:** No — not needed for MVP. Export always exports all secrets for the environment.
3. **`.envrc` integration:** Yes — zuul will document the recommended direnv hook pattern.
4. **Dry-run mode:** Yes — destructive commands support `--dry-run`, which lists all resources that would be affected (e.g., `env delete --force --dry-run` lists all secrets that would be deleted along with the environment).
5. **Import command:** Yes — MVP includes `zuul import`.

---

## 14. Open Questions

1. **Environment rename atomicity:** Renaming an environment requires renaming N GCP secrets. Should zuul track rename progress (e.g., in a `zuul__registry__migrations` secret) to handle interrupted renames, or is the idempotent re-run approach sufficient?
2. ~~**Metadata scoping:** Should metadata be per secret+environment (current design), per logical secret (shared across environments), or both?~~ **Resolved:** Metadata is shared across environments by default. Set/delete operations apply to all environments where the secret exists. The `--env` flag can scope to a single environment when needed. Stored as GCP annotations on each underlying secret, kept in sync at the CLI layer.
3. **`zuul diff` value display:** Should diff show full values, truncated values, or just indicate "differs" / "matches"? Full values may be sensitive; truncated values may not be useful.

---

## Appendix A: Example Workflows

### A.1 Initial Setup

```bash
# Install zuul (assumed available via cargo)
cargo install zuul

# Initialize project
cd my-project
zuul init --project my-gcp-project-123

# Create environments
zuul env create production --description "Live production"
zuul env create staging --description "Pre-production staging"
zuul env create dev --description "Local development"
```

### A.2 Developer Onboarding

```bash
# Developer clones a repo that already has .zuul.toml
git clone git@github.com:company/my-project.git
cd my-project

# Set up authentication (one-time)
zuul auth
# → Walks through gcloud auth, validates access, shows available environments

# Start working with dev secrets immediately
zuul run --env dev -- cargo run
```

### A.3 Local Overrides

```bash
# Create a local overrides file (automatically gitignored by zuul init)
cat > .zuul.local.toml << 'EOF'
[secrets]
DATABASE_URL = "postgres://localhost:5432/mydb_local"
REDIS_URL = "redis://localhost:6379"
EOF

# Run with local overrides applied on top of backend secrets
zuul run --env dev -- cargo run
# → DATABASE_URL uses local value, other secrets come from backend

# Verify what the real backend has (skip local overrides)
zuul run --env dev --no-local -- env | grep DATABASE_URL
# → Shows the team-wide dev value from GCP
```

### A.4 Managing Secrets

```bash
# Set a secret for dev
zuul secret set DATABASE_URL --env dev "postgres://localhost:5432/mydb"

# Set the same secret for production (prompted for value interactively)
zuul secret set DATABASE_URL --env production

# Set a multi-line value from a file
zuul secret set TLS_CERT --env production --from-file ./certs/server.pem

# View which environments have DATABASE_URL
zuul secret info DATABASE_URL

# Add metadata (shared across all environments where DATABASE_URL exists)
zuul secret metadata set DATABASE_URL rotate-by "2026-06-01"
zuul secret metadata set DATABASE_URL owner "backend-team"
```

### A.5 Developer Workflow

```bash
# Export dev secrets to .envrc for direnv
zuul export --env dev --format direnv --output .envrc
direnv allow

# Or run directly with secrets injected
zuul run --env dev -- cargo run
```

### A.6 Migrating from an Existing .env File

```bash
# Preview what would be imported
zuul import --env dev --file .env.local --dry-run

# Import all secrets
zuul import --env dev --file .env.local

# Copy a secret from dev to staging
zuul secret copy DATABASE_URL --from dev --to staging
```

### A.7 CI/CD Pipeline

```bash
# In a CI/CD pipeline (using service account credentials)
export ZUUL_GCP_CREDENTIALS=/secrets/ci-sa-key.json

# Export as dotenv for the build
zuul export --env production --format dotenv --output .env

# Or run the deployment command with secrets
zuul run --env production -- ./deploy.sh
```
