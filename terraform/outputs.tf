output "project_id" {
  description = "GCP project ID where zuul secrets are managed."
  value       = var.project_id
}

output "secret_manager_api_enabled" {
  description = "Confirms that the Secret Manager API has been enabled on the project."
  value       = google_project_service.secret_manager.service
}
