use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};

pub(crate) struct WorkflowPublicationStore;
impl WorkflowPublicationStore {
    pub fn publish(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
        input: &WorkflowPublish,
    ) -> Result<WorkflowVersion, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "expected workflow revision must be positive",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::WorkflowsPublish, project)?;
        if let Some(project) = project {
            PlatformAuthority::project(&mut tx, project)?;
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "workflow.publish",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(w) FROM workflows w WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2 FOR UPDATE",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let current: WorkflowDefinition = PlatformStore::decode(value)?;
        if current.revision != input.expected_revision {
            return Err(PlatformError::Conflict("workflow revision changed"));
        }
        if current.archived {
            return Err(PlatformError::Conflict("workflow is archived"));
        }
        current.draft.validate()?;
        current
            .draft
            .validate_resource_kinds(&current.resource_kinds)?;
        crate::workflow_definition_store::WorkflowDefinitionStore::kinds(&current.resource_kinds)?;
        let content = json!({"name":current.name,"purpose":current.purpose,"resource_kinds":current.resource_kinds,"graph":current.draft});
        let bytes = serde_json::to_vec(&content).map_err(|_| PlatformError::Storage)?;
        let checksum = SecretGenerator::checksum(&bytes);
        let version = SecretGenerator::id("wfv")?;
        let number = current
            .latest_version_number
            .checked_add(1)
            .ok_or(PlatformError::Conflict("workflow version exhausted"))?;
        let value:Value=tx.query_one("INSERT INTO workflow_versions AS v(id,workflow_id,project_id,number,name,purpose,resource_kinds,graph,checksum,reason,published_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING to_jsonb(v)",&[&version,&id,&project,&number,&current.name,&current.purpose,&content["resource_kinds"],&content["graph"],&checksum,&reason,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.get(0);
        tx.execute("UPDATE workflows SET latest_version_id=$3,latest_version_number=$4,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2",&[&id,&project,&version,&number]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "workflow.published",
            Some(id),
            json!({"project_id":project,"version_id":version,"number":number,"checksum":checksum,"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
