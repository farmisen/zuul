variable "project_id" {
  description = "GCP project ID where Secret Manager will be enabled and zuul secrets will be stored."
  type        = string

  validation {
    condition     = can(regex("^[a-z][a-z0-9-]{4,28}[a-z0-9]$", var.project_id))
    error_message = "project_id must be a valid GCP project ID (6-30 chars, lowercase, digits, hyphens, starts with letter, ends with letter or digit)."
  }
}

variable "environments" {
  description = "List of zuul environment names to provision. Used by IAM bindings in the iam module."
  type        = list(string)
  default     = ["production", "staging", "dev"]

  validation {
    condition     = length(var.environments) > 0
    error_message = "At least one environment must be specified."
  }

  validation {
    condition     = alltrue([for e in var.environments : can(regex("^[a-z0-9][a-z0-9-]*$", e))])
    error_message = "Environment names must match [a-z0-9][a-z0-9-]* (lowercase, alphanumeric, hyphens, no leading hyphen)."
  }
}

variable "environment_descriptions" {
  description = "Optional map of environment name to description. Environments not in this map will have no description."
  type        = map(string)
  default     = {}
}

variable "members" {
  description = <<-EOT
    Map of IAM member to their access configuration. Members use the GCP IAM
    format: "user:alice@company.com", "serviceAccount:ci@project.iam", "group:team@company.com".

    Roles:
      - "admin" — full secretmanager.admin access (project-wide, environments ignored)
      - "write" — read + write secrets in specified environments
      - "read"  — read-only access to secrets in specified environments
  EOT
  type = map(object({
    role         = string
    environments = optional(list(string), [])
  }))

  validation {
    condition     = alltrue([for _, m in var.members : contains(["admin", "write", "read"], m.role)])
    error_message = "Member role must be one of: admin, write, read."
  }

  validation {
    condition     = alltrue([for _, m in var.members : m.role == "admin" || length(m.environments) > 0])
    error_message = "Non-admin members must specify at least one environment."
  }
}

variable "service_accounts" {
  description = "Map of service account name to zuul environment name. Creates a dedicated service account scoped to that environment's secrets. For CI/CD pipelines."
  type        = map(string)
  default     = {}

  validation {
    condition     = alltrue([for name, _ in var.service_accounts : can(regex("^[a-z][a-z0-9-]{4,28}[a-z0-9]$", name))])
    error_message = "Service account names must be 6-30 chars, lowercase, digits, hyphens, start with letter, end with letter or digit."
  }
}

variable "create_developer_sas" {
  description = "Create per-developer service accounts mirroring existing IAM access from the members variable. Useful for developers with multiple GCP accounts who need a dedicated key file."
  type        = bool
  default     = false
}
