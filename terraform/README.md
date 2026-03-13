# Zuul Terraform Module

Provisions the GCP infrastructure required by [Zuul](../README.md) — a CLI tool for managing secrets across environments backed by Google Cloud Secret Manager.

## What This Module Does

- Enables the Secret Manager API on your GCP project
- Grants `secretmanager.admin` to designated admin users
- Creates scoped read-only IAM bindings per zuul environment
- Grants all accessors read access to the zuul environment registry
- Optionally creates dedicated service accounts scoped to specific environments

## Prerequisites

- [Terraform](https://www.terraform.io/) >= 1.5
- A GCP project with billing enabled
- Authenticated `gcloud` CLI or service account credentials

## Usage

### Minimal Setup

```hcl
module "zuul" {
  source       = "./terraform"
  project_id   = "my-gcp-project"
  admin_emails = ["ops@company.com"]
}
```

### With Per-Environment Accessors

```hcl
module "zuul" {
  source       = "./terraform"
  project_id   = "my-gcp-project"
  admin_emails = ["ops@company.com"]

  environment_accessors = {
    dev        = ["user:alice@company.com", "user:bob@company.com"]
    staging    = ["user:alice@company.com"]
    production = []
  }
}
```

### With Service Accounts

```hcl
module "zuul" {
  source       = "./terraform"
  project_id   = "my-gcp-project"
  admin_emails = ["ops@company.com"]

  service_accounts = {
    "staging-ci"        = "staging"
    "production-api"    = "production"
    "production-worker" = "production"
  }
}
```

This creates three service accounts (`zuul-staging-ci`, `zuul-production-api`, `zuul-production-worker`), each scoped to their target environment's secrets plus registry read access.

### Full Example

```hcl
module "zuul" {
  source       = "./terraform"
  project_id   = "my-gcp-project"
  environments = ["production", "staging", "dev"]
  admin_emails = ["ops@company.com", "lead@company.com"]

  environment_accessors = {
    dev     = ["user:alice@company.com", "user:bob@company.com"]
    staging = ["user:alice@company.com"]
  }

  service_accounts = {
    "production-deploy" = "production"
    "staging-ci"        = "staging"
  }
}

output "sa_emails" {
  value = module.zuul.service_account_emails
}
```

## Variables

| Name | Type | Required | Default | Description |
|------|------|----------|---------|-------------|
| `project_id` | `string` | Yes | — | GCP project ID |
| `environments` | `list(string)` | No | `["production", "staging", "dev"]` | Zuul environment names |
| `admin_emails` | `list(string)` | Yes | — | Emails granted `secretmanager.admin` |
| `environment_accessors` | `map(list(string))` | No | `{}` | Map of environment to IAM members with read-only access |
| `service_accounts` | `map(string)` | No | `{}` | Map of SA name to environment (creates scoped SAs) |

## Outputs

| Name | Description |
|------|-------------|
| `project_id` | GCP project ID |
| `secret_manager_api_enabled` | The Secret Manager API service name |
| `service_account_emails` | Map of SA name to email for each created service account |

## IAM Model

This module uses `google_project_iam_member` (additive) for all bindings — it will never remove existing IAM policies on your project.

### Access Scoping

- **Project access** — all admins and environment accessors automatically receive `roles/browser` on the project, so no separate project membership step is needed
- **Admins** get full `roles/secretmanager.admin` on the project
- **Environment accessors** get `roles/secretmanager.secretAccessor` with an IAM condition restricting access to secrets matching `zuul__{env}__*`
- **All non-admin members** automatically get read access to the `zuul__registry` secret (required to list environments)
- **Service accounts** get the same scoped access as environment accessors

## Getting Started

1. Copy `terraform.tfvars.example` to `terraform.tfvars` and fill in your values
2. Run `terraform init`
3. Run `terraform plan` to review changes
4. Run `terraform apply`
5. Initialize zuul: `zuul init --project <your-project-id>`
