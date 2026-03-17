# File Backend

The file backend stores all environments, secrets, and metadata in a single encrypted JSON file using `age` passphrase encryption. Designed for local development, small projects, and offline use.

## Setup

```bash
zuul init --backend file
```

This creates:
- `.zuul.toml` with `type = "file"`
- `.zuul.secrets.enc` — the encrypted store (added to `.gitignore`)

## Configuration

```toml
[backend]
type = "file"
# path = ".zuul.secrets.enc"    # default store file path

[defaults]
environment = "dev"
```

## Passphrase Resolution

The passphrase is resolved in this order:

1. `ZUUL_PASSPHRASE` env var — for scripting and CI (no prompt)
2. `ZUUL_KEY_FILE` env var — path to an age identity file (overrides config)
3. `identity` field in `.zuul.toml` — age identity file on disk
4. Interactive prompt — fallback for interactive use (hidden input)

For CI/CD pipelines, set `ZUUL_PASSPHRASE` in your platform's secrets UI.

## Environment Management

Unlike the GCP backend (where environments are managed by Terraform), the file backend supports full environment CRUD via the CLI:

```bash
zuul env create production --description "Live production"
zuul env create staging --description "Pre-production"
zuul env list
zuul env update staging --description "Updated description"
zuul env delete staging --force
```

## Storage Format

The encrypted file contains a JSON document:

```json
{
  "version": 1,
  "environments": {
    "dev": { "description": "Development", "created_at": "...", "updated_at": "..." }
  },
  "secrets": {
    "dev": {
      "DATABASE_URL": { "value": "postgres://...", "version": 1, "created_at": "...", "updated_at": "..." }
    }
  },
  "metadata": {
    "dev": {
      "DATABASE_URL": { "owner": "backend-team" }
    }
  }
}
```

This JSON is encrypted with `age` before writing to disk. The file is never stored in plaintext.

## Concurrency

File locking via `flock` prevents concurrent corruption when multiple processes access the store simultaneously. Each operation acquires an exclusive lock on a `.lock` sidecar file.

## Limitations

Compared to cloud backends (GCP, AWS):

- **Single machine** — The encrypted file lives on one machine. No built-in sync across team members.
- **No IAM** — Access control is filesystem-level only. Anyone with the passphrase and file can read all secrets.
- **No audit trail** — No built-in logging of who accessed or modified secrets.
- **No versioning** — Secret values are overwritten in place (the `version` counter increments, but old values are not retained).

For team use with access control, use the [GCP backend](backend-gcp.md) instead.
