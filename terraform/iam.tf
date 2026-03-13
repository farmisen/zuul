locals {
  # Flatten environment_accessors into a list of {env, member} pairs for for_each
  accessor_bindings = flatten([
    for env, members in var.environment_accessors : [
      for member in members : {
        env    = env
        member = member
      }
    ]
  ])

  # Deduplicate members across environments for registry read access
  registry_readers = distinct(flatten(values(var.environment_accessors)))
}

# --- Admin bindings ---

resource "google_project_iam_member" "admin" {
  for_each = toset(var.admin_emails)

  project = var.project_id
  role    = "roles/secretmanager.admin"
  member  = "user:${each.value}"
}

# --- Per-environment accessor bindings ---

resource "google_project_iam_member" "env_accessor" {
  for_each = {
    for binding in local.accessor_bindings :
    "${binding.env}--${binding.member}" => binding
  }

  project = var.project_id
  role    = "roles/secretmanager.secretAccessor"
  member  = each.value.member

  condition {
    title       = "zuul-${each.value.env}-secrets"
    description = "Access to zuul secrets in the ${each.value.env} environment"
    expression  = "resource.name.startsWith(\"projects/${var.project_id}/secrets/zuul__${each.value.env}__\")"
  }
}

# --- Registry read access for all environment accessors ---

resource "google_project_iam_member" "registry_reader" {
  for_each = toset(local.registry_readers)

  project = var.project_id
  role    = "roles/secretmanager.secretAccessor"
  member  = each.value

  condition {
    title       = "zuul-registry-read"
    description = "Read access to the zuul environment registry"
    expression  = "resource.name == \"projects/${var.project_id}/secrets/zuul__registry\""
  }
}

# --- Service accounts ---

resource "google_service_account" "zuul" {
  for_each = var.service_accounts

  project      = var.project_id
  account_id   = "zuul-${each.key}"
  display_name = "Zuul SA for ${each.value} environment (${each.key})"
}

resource "google_project_iam_member" "sa_env_accessor" {
  for_each = var.service_accounts

  project = var.project_id
  role    = "roles/secretmanager.secretAccessor"
  member  = "serviceAccount:${google_service_account.zuul[each.key].email}"

  condition {
    title       = "zuul-${each.value}-secrets"
    description = "Access to zuul secrets in the ${each.value} environment"
    expression  = "resource.name.startsWith(\"projects/${var.project_id}/secrets/zuul__${each.value}__\")"
  }
}

resource "google_project_iam_member" "sa_registry_reader" {
  for_each = var.service_accounts

  project = var.project_id
  role    = "roles/secretmanager.secretAccessor"
  member  = "serviceAccount:${google_service_account.zuul[each.key].email}"

  condition {
    title       = "zuul-registry-read"
    description = "Read access to the zuul environment registry"
    expression  = "resource.name == \"projects/${var.project_id}/secrets/zuul__registry\""
  }
}
