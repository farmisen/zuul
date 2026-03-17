terraform {
  required_version = ">= 1.5"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = ">= 5.0"
    }
  }
}

provider "google" {
  project = var.project_id
}

resource "google_project_service" "secret_manager" {
  project            = var.project_id
  service            = "secretmanager.googleapis.com"
  disable_on_destroy = false
}

# --- Zuul environment registry ---

# Terraform owns the environment registry — `terraform apply` is the single
# source of truth for which environments exist and their IAM bindings.
#
# `plantimestamp()` (Terraform ≥ 1.5) is evaluated once at plan time and
# stays constant across the apply, so it does NOT cause perpetual drift
# the way `timestamp()` would.  The timestamps here are informational —
# they record when the plan that last touched the environment was created.
locals {
  registry_json = jsonencode({
    version = 1
    environments = {
      for env in var.environments : env => {
        description = lookup(var.environment_descriptions, env, null)
        created_at  = plantimestamp()
        updated_at  = plantimestamp()
      }
    }
  })
}

resource "google_secret_manager_secret" "registry" {
  project   = var.project_id
  secret_id = "zuul__registry"

  labels = {
    "zuul-managed" = "true"
  }

  replication {
    auto {}
  }

  depends_on = [google_project_service.secret_manager]
}

resource "google_secret_manager_secret_version" "registry" {
  secret      = google_secret_manager_secret.registry.id
  secret_data = local.registry_json
}
