# --- Locals: derive bindings from the unified members variable ---

locals {
  # Separate members by role
  admins = { for member, cfg in var.members : member => cfg if cfg.role == "admin" }
  writers = { for member, cfg in var.members : member => cfg if cfg.role == "write" }
  readers = { for member, cfg in var.members : member => cfg if cfg.role == "read" }

  # Flatten writer + reader per-env bindings into {member, env, role} triples
  env_bindings = flatten([
    for member, cfg in var.members : [
      for env in cfg.environments : {
        member = member
        env    = env
        role   = cfg.role
      }
    ] if cfg.role != "admin"
  ])

  # All non-admin members need registry read access
  registry_readers = [for member, cfg in var.members : member if cfg.role != "admin"]

  # All members need project-level browser access
  all_members = keys(var.members)

  # Developer SA: derive SA name from member key (e.g., "user:alice@co.com" → "alice")
  developer_sa_members = var.create_developer_sas ? {
    for member, cfg in var.members : member => cfg
    if startswith(member, "user:")
  } : {}

  # Derive unique SA name from email: "user:alice@company.com" → "alice-company"
  # Includes domain (without TLD) to avoid collisions when the same username
  # exists across different domains.
  developer_sa_names = {
    for member, cfg in local.developer_sa_members : member => substr(
      join("-", [
        replace(regex("^user:([^@]+)@", member)[0], ".", "-"),
        replace(regex("@([^.]+)\\.", member)[0], ".", "-"),
      ]),
      0, 21  # Max 21 chars — "zuul-dev-" prefix adds 9, total max 30
    )
  }

  # Flatten developer SA per-env bindings
  dev_sa_env_bindings = flatten([
    for member, cfg in local.developer_sa_members : [
      for env in(cfg.role == "admin" ? var.environments : cfg.environments) : {
        member  = member
        sa_name = local.developer_sa_names[member]
        env     = env
        role    = cfg.role
      }
    ]
  ])
}

# --- Project-level browser access for all members ---

resource "google_project_iam_member" "project_browser" {
  for_each = toset(local.all_members)

  project = var.project_id
  role    = "roles/browser"
  member  = each.value
}

# --- Admin bindings (project-wide secretmanager.admin) ---

resource "google_project_iam_member" "admin" {
  for_each = local.admins

  project = var.project_id
  role    = "roles/secretmanager.admin"
  member  = each.key
}

# --- Per-environment accessor bindings (read) ---

resource "google_project_iam_member" "env_accessor" {
  for_each = {
    for binding in local.env_bindings :
    "${binding.member}--${binding.env}--accessor" => binding
  }

  project = var.project_id
  role    = "roles/secretmanager.secretAccessor"
  member  = each.value.member

  condition {
    title       = "zuul-${each.value.env}-secrets-read"
    description = "Read access to zuul secrets in the ${each.value.env} environment"
    expression  = "resource.name.startsWith(\"projects/${var.project_id}/secrets/zuul__${each.value.env}__\")"
  }
}

# --- Per-environment writer bindings (secretVersionManager for write role) ---

resource "google_project_iam_member" "env_writer" {
  for_each = {
    for binding in local.env_bindings :
    "${binding.member}--${binding.env}--writer" => binding
    if binding.role == "write"
  }

  project = var.project_id
  role    = "roles/secretmanager.secretVersionManager"
  member  = each.value.member

  condition {
    title       = "zuul-${each.value.env}-secrets-write"
    description = "Write access to zuul secrets in the ${each.value.env} environment"
    expression  = "resource.name.startsWith(\"projects/${var.project_id}/secrets/zuul__${each.value.env}__\")"
  }
}

# --- Registry read access for non-admin members ---

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

# --- CI/CD Service accounts ---

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

# --- Developer service accounts ---

resource "google_service_account" "developer" {
  for_each = local.developer_sa_names

  project      = var.project_id
  account_id   = "zuul-dev-${each.value}"
  display_name = "Zuul developer SA for ${each.value}"
}

# Developer SAs get project-level browser access (needed for zuul audit)
resource "google_project_iam_member" "dev_sa_browser" {
  for_each = local.developer_sa_names

  project = var.project_id
  role    = "roles/browser"
  member  = "serviceAccount:${google_service_account.developer[each.key].email}"
}

# Admin developer SAs get project-wide secretmanager.admin
resource "google_project_iam_member" "dev_sa_admin" {
  for_each = {
    for member, cfg in local.developer_sa_members : member => cfg
    if cfg.role == "admin"
  }

  project = var.project_id
  role    = "roles/secretmanager.admin"
  member  = "serviceAccount:${google_service_account.developer[each.key].email}"
}

# Non-admin developer SAs get per-env accessor
resource "google_project_iam_member" "dev_sa_env_accessor" {
  for_each = {
    for binding in local.dev_sa_env_bindings :
    "${binding.member}--${binding.env}--accessor" => binding
    if binding.role != "admin"
  }

  project = var.project_id
  role    = "roles/secretmanager.secretAccessor"
  member  = "serviceAccount:${google_service_account.developer[each.value.member].email}"

  condition {
    title       = "zuul-${each.value.env}-secrets-read"
    description = "Read access to zuul secrets in the ${each.value.env} environment"
    expression  = "resource.name.startsWith(\"projects/${var.project_id}/secrets/zuul__${each.value.env}__\")"
  }
}

# Write-role developer SAs also get secretVersionManager
resource "google_project_iam_member" "dev_sa_env_writer" {
  for_each = {
    for binding in local.dev_sa_env_bindings :
    "${binding.member}--${binding.env}--writer" => binding
    if binding.role == "write"
  }

  project = var.project_id
  role    = "roles/secretmanager.secretVersionManager"
  member  = "serviceAccount:${google_service_account.developer[each.value.member].email}"

  condition {
    title       = "zuul-${each.value.env}-secrets-write"
    description = "Write access to zuul secrets in the ${each.value.env} environment"
    expression  = "resource.name.startsWith(\"projects/${var.project_id}/secrets/zuul__${each.value.env}__\")"
  }
}

# Developer SAs need registry read access
resource "google_project_iam_member" "dev_sa_registry_reader" {
  for_each = local.developer_sa_names

  project = var.project_id
  role    = "roles/secretmanager.secretAccessor"
  member  = "serviceAccount:${google_service_account.developer[each.key].email}"

  condition {
    title       = "zuul-registry-read"
    description = "Read access to the zuul environment registry"
    expression  = "resource.name == \"projects/${var.project_id}/secrets/zuul__registry\""
  }
}
