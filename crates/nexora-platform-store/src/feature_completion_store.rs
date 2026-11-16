use crate::{
    PlatformStore, audit_writer::AuditWriter, feature_governance_policy::FeatureGovernancePolicy,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn complete_feature(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &FeatureComplete,
    ) -> Result<Value, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation("invalid feature revision"));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        FeatureGovernancePolicy::read(&mut tx, ctx, project)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::DocumentsWrite, Some(project))?;
        tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('manager','super_admin') FOR SHARE", &[&ctx.actor_id()]).map_err(Self::database_error)?.ok_or(PlatformError::Forbidden)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "feature.complete",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        FeatureGovernancePolicy::catalog(&mut tx, project)?;
        let before: Value = tx.query_opt("SELECT to_jsonb(f) FROM project_features f WHERE id=$1 AND project_id=$2 FOR UPDATE", &[&id, &project]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        if before["revision"].as_i64() != Some(input.expected_revision) {
            return Err(PlatformError::Conflict("feature revision changed"));
        }
        if before["archived"] == true {
            return Err(PlatformError::Conflict("feature is archived"));
        }
        if before["status"] == "completed" {
            return Err(PlatformError::Conflict("feature is already completed"));
        }
        let value: Value = tx.query_one("UPDATE project_features f SET status='completed',completed_at=clock_timestamp(),completed_by=$3,revision=revision+1 WHERE id=$1 AND project_id=$2 RETURNING to_jsonb(f)", &[&id, &project, &ctx.actor_id()]).map_err(Self::database_error)?.get(0);
        tx.execute(
            "UPDATE feature_catalogs SET revision=revision+1 WHERE project_id=$1",
            &[&project],
        )
        .map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "feature.complete",
            Some(id),
            json!({"project_id":project,"before":before,"after":value}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
