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

variable "admin_emails" {
  description = "Email addresses of users or service accounts that should receive secretmanager.admin role."
  type        = list(string)

  validation {
    condition     = length(var.admin_emails) > 0
    error_message = "At least one admin email must be specified."
  }
}

variable "environment_accessors" {
  description = "Map of zuul environment name to list of IAM members (e.g. user:alice@co.com, serviceAccount:ci@proj.iam.gserviceaccount.com) that should receive read-only access to that environment's secrets."
  type        = map(list(string))
  default     = {}
}

variable "service_accounts" {
  description = "Map of service account name to zuul environment name. Creates a dedicated service account scoped to that environment's secrets. Multiple SAs can target the same environment."
  type        = map(string)
  default     = {}

  validation {
    condition     = alltrue([for name, _ in var.service_accounts : can(regex("^[a-z][a-z0-9-]{4,28}[a-z0-9]$", name))])
    error_message = "Service account names must be 6-30 chars, lowercase, digits, hyphens, start with letter, end with letter or digit."
  }
}
