output "project_id" {
  description = "GCP project ID where zuul secrets are managed."
  value       = var.project_id
}

output "secret_manager_api_enabled" {
  description = "Confirms that the Secret Manager API has been enabled on the project."
  value       = google_project_service.secret_manager.service
}

output "registry_secret_id" {
  description = "The GCP secret ID of the zuul environment registry."
  value       = google_secret_manager_secret.registry.secret_id
}

output "environments" {
  description = "List of zuul environments provisioned in the registry."
  value       = var.environments
}

output "service_account_emails" {
  description = "Map of CI/CD service account name to email address."
  value = {
    for name, sa in google_service_account.zuul : name => sa.email
  }
}

output "developer_service_account_emails" {
  description = "Map of member identity to their developer service account email."
  value = {
    for member, sa in google_service_account.developer : member => sa.email
  }
}
