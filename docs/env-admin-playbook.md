# Environment Admin Playbook

Step-by-step runbook for common environment lifecycle operations. All mutating operations go through Terraform — the CLI is read-only for environments.

## Prerequisites

- Terraform >= 1.5 installed
- Access to `terraform/terraform.tfvars` (or create one from `terraform.tfvars.example`)
- GCP credentials with `secretmanager.admin` role

---

## Creating a New Environment

1. Add the environment to `terraform/terraform.tfvars`:

   ```hcl
   environments = ["production", "staging", "dev", "qa"]  # add "qa"

   environment_descriptions = {
     production = "Live production"
     staging    = "Pre-production staging"
     dev        = "Local development"
     qa         = "QA testing"  # add description
   }
   ```

2. (Optional) Add IAM accessors for the new environment:

   ```hcl
   environment_accessors = {
     qa = ["serviceAccount:ci-qa@project.iam.gserviceaccount.com"]
   }
   ```

3. Apply:

   ```bash
   cd terraform
   terraform plan   # review changes
   terraform apply  # creates env in registry + IAM bindings
   ```

4. Verify from the CLI:

   ```bash
   zuul env list
   zuul env show qa
   ```

---

## Renaming an Environment

Renaming is a two-step process: create the new name, migrate secrets, remove the old name.

1. Add the new name and keep the old name in `terraform/terraform.tfvars`:

   ```hcl
   environments = ["production", "staging", "dev", "staging-v2"]
   ```

2. Apply to create the new environment and its IAM bindings:

   ```bash
   cd terraform && terraform apply
   ```

3. Copy secrets from old to new:

   ```bash
   zuul env copy staging staging-v2 --force
   ```

4. Verify the new environment:

   ```bash
   zuul secret list --env staging-v2
   ```

5. Remove the old environment from `terraform/terraform.tfvars`:

   ```hcl
   environments = ["production", "dev", "staging-v2"]
   ```

6. Drain secrets from the old environment and apply:

   ```bash
   zuul env drain staging --force
   cd terraform && terraform apply
   ```

---

## Decommissioning an Environment

1. Drain all secrets:

   ```bash
   zuul env drain staging --force
   ```

2. Remove from `terraform/terraform.tfvars`:

   ```hcl
   environments = ["production", "dev"]  # removed "staging"
   ```

3. Apply:

   ```bash
   cd terraform && terraform apply
   ```

   This removes the environment from the registry and revokes its IAM bindings.

4. **Recovery window**: If secrets were deleted by mistake, GCP Secret Manager retains deleted secrets for a configurable grace period. Use `gcloud` to undelete:

   ```bash
   gcloud secrets list --filter="labels.zuul-env=staging" --include-deleted
   gcloud secrets versions access latest --secret=zuul__staging__DB_URL
   ```

---

## Terraform Pre-Destroy Hook

To automatically drain secrets before Terraform removes an environment, add a `null_resource` to your Terraform configuration:

```hcl
resource "null_resource" "drain_env" {
  for_each = toset(var.environments)

  triggers = {
    env = each.key
  }

  provisioner "local-exec" {
    when    = destroy
    command = "zuul env drain ${self.triggers.env} --force"
  }
}
```

This ensures bound secrets are cleaned up before the environment is removed from the registry.

---

## Recovering from Drift

If the Terraform state and the `zuul__registry` secret diverge (e.g., someone manually edited the registry):

1. Check current state:

   ```bash
   zuul env list                    # what the registry says
   cd terraform && terraform plan   # what Terraform expects
   ```

2. If Terraform plan shows the registry will be updated, review and apply:

   ```bash
   terraform apply
   ```

   Terraform overwrites the registry to match the declared `environments` variable.

3. If secrets exist for an environment not in Terraform, they are orphaned but safe — GCP still stores them. Add the environment back to Terraform or drain and remove the secrets manually.

---

## Rotating IAM Bindings

To rotate a service account's access:

1. Update `terraform/terraform.tfvars` with the new service account:

   ```hcl
   environment_accessors = {
     production = ["serviceAccount:new-ci@project.iam.gserviceaccount.com"]
   }
   ```

2. Apply:

   ```bash
   cd terraform && terraform apply
   ```

   The old binding is removed and the new one is created atomically.
