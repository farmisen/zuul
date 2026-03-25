# Zuul Terraform Module

Provisions the GCP infrastructure required by [Zuul](../README.md) — a CLI tool for managing secrets across environments backed by Google Cloud Secret Manager.

## What This Module Does

- Enables the Secret Manager API on your GCP project
- Creates the zuul environment registry with your configured environments
- Grants IAM bindings based on member roles (admin, write, read)
- Scopes non-admin access per zuul environment using IAM conditions
- Grants all non-admin members read access to the zuul environment registry
- Optionally creates dedicated service accounts for CI/CD pipelines
- Optionally creates per-developer service accounts mirroring their IAM access

## Prerequisites

- [Terraform](https://www.terraform.io/) >= 1.5
- A GCP project with billing enabled
- Authenticated `gcloud` CLI or service account credentials

## Usage

### Minimal Setup

```hcl
module "zuul" {
  source     = "./terraform"
  project_id = "my-gcp-project"

  members = {
    "user:ops@company.com" = { role = "admin" }
  }
}
```

### With Per-Environment Access

```hcl
module "zuul" {
  source     = "./terraform"
  project_id = "my-gcp-project"

  members = {
    "user:ops@company.com" = {
      role = "admin"
    }
    "user:alice@company.com" = {
      role         = "write"
      environments = ["dev", "staging"]
    }
    "user:bob@company.com" = {
      role         = "read"
      environments = ["dev"]
    }
  }
}
```

### With Service Accounts

```hcl
module "zuul" {
  source     = "./terraform"
  project_id = "my-gcp-project"

  members = {
    "user:ops@company.com" = { role = "admin" }
  }

  service_accounts = {
    "staging-ci"        = "staging"
    "production-api"    = "production"
    "production-worker" = "production"
  }
}
```

This creates three service accounts (`zuul-staging-ci`, `zuul-production-api`, `zuul-production-worker`), each scoped to their target environment's secrets plus registry read access.

### With Developer Service Accounts

For teams where developers have multiple GCP accounts on one machine:

```hcl
module "zuul" {
  source     = "./terraform"
  project_id = "my-gcp-project"

  members = {
    "user:ops@company.com" = { role = "admin" }
    "user:alice@company.com" = {
      role         = "write"
      environments = ["dev", "staging"]
    }
  }

  create_developer_sas = true
}
```

This creates per-developer service accounts (`zuul-dev-ops`, `zuul-dev-alice`) mirroring each `user:` member's role and environment access. Developers download their key from the GCP Console (IAM → Service Accounts → Keys) and configure it with `zuul auth`.

### Full Example

```hcl
module "zuul" {
  source       = "./terraform"
  project_id   = "my-gcp-project"
  environments = ["production", "staging", "dev"]

  environment_descriptions = {
    production = "Live production environment"
    staging    = "Pre-production staging"
    dev        = "Local development"
  }

  members = {
    "user:ops@company.com" = { role = "admin" }
    "user:lead@company.com" = { role = "admin" }
    "user:alice@company.com" = {
      role         = "write"
      environments = ["dev", "staging"]
    }
    "user:bob@company.com" = {
      role         = "read"
      environments = ["dev"]
    }
  }

  create_developer_sas = true

  service_accounts = {
    "production-deploy" = "production"
    "staging-ci"        = "staging"
  }
}

output "sa_emails" {
  value = module.zuul.service_account_emails
}

output "dev_sa_emails" {
  value = module.zuul.developer_service_account_emails
}
```

## Variables

| Name | Type | Required | Default | Description |
|------|------|----------|---------|-------------|
| `project_id` | `string` | Yes | — | GCP project ID |
| `environments` | `list(string)` | No | `["production", "staging", "dev"]` | Zuul environment names |
| `environment_descriptions` | `map(string)` | No | `{}` | Optional descriptions for environments |
| `members` | `map(object)` | Yes | — | Map of IAM member to `{role, environments}`. Roles: `admin`, `write`, `read` |
| `service_accounts` | `map(string)` | No | `{}` | Map of SA name to environment (creates scoped CI/CD SAs) |
| `create_developer_sas` | `bool` | No | `false` | Create per-developer SAs mirroring their IAM access |

## Outputs

| Name | Description |
|------|-------------|
| `project_id` | GCP project ID |
| `secret_manager_api_enabled` | The Secret Manager API service name |
| `registry_secret_id` | GCP secret ID of the zuul environment registry |
| `environments` | List of zuul environments provisioned in the registry |
| `service_account_emails` | Map of CI/CD SA name to email |
| `developer_service_account_emails` | Map of member identity to developer SA email |

## IAM Model

This module uses `google_project_iam_member` (additive) for all bindings — it will never remove existing IAM policies on your project.

### Roles

| Zuul role | GCP IAM roles | Scope |
|-----------|--------------|-------|
| `admin` | `secretmanager.admin` | Project-wide (all secrets) |
| `write` | `secretmanager.secretAccessor` + `secretmanager.secretVersionManager` | Per-environment (IAM condition) |
| `read` | `secretmanager.secretAccessor` | Per-environment (IAM condition) |

### Access Scoping

- **Project access** — all members receive `roles/browser` on the project
- **Admins** get full `roles/secretmanager.admin` on the project
- **Write members** get `secretAccessor` + `secretVersionManager` with IAM conditions restricting access to `zuul__{env}__*`
- **Read members** get `secretAccessor` with the same environment-scoped conditions
- **All non-admin members** automatically get read access to the `zuul__registry` secret
- **CI/CD service accounts** get the same scoped read access as `read` members
- **Developer SAs** mirror the exact role and environment access of their `user:` member

## Getting Started

1. Copy `terraform.tfvars.example` to `terraform.tfvars` and fill in your values
2. Run `terraform init`
3. Run `terraform plan` to review changes
4. Run `terraform apply`
5. Initialize zuul: `zuul init --project <your-project-id>`
6. Authenticate: `zuul auth` (choose ADC login or configure SA key file)

Environments are created in the registry by `terraform apply` — no need to run `zuul env create` separately.

> **Note:** The registry secret version uses `ignore_changes` on `secret_data`, so subsequent `zuul env create` or `zuul env delete` commands won't be overwritten by Terraform. The initial `terraform apply` seeds the registry; Zuul manages it from there.
