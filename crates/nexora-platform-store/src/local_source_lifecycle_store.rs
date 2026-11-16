use crate::{
    PlatformStore, audit_writer::AuditWriter,
    local_source_operator_authority::LocalSourceOperatorAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::{
    AuthContext, DomainText, LocalRepositorySourceInfo, LocalSourceChange, PlatformError,
};
use serde_json::{Value, json};

pub(crate) struct LocalSourceLifecycleStore;
impl LocalSourceLifecycleStore {
    pub fn revoke(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        input: &LocalSourceChange,
    ) -> Result<LocalRepositorySourceInfo, PlatformError> {
        let request = store
            .request_context
            .clone()
            .ok_or(PlatformError::Validation(
                "local revocation requires an idempotency key",
            ))?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        LocalSourceOperatorAuthority::lock(&mut tx, ctx, project)?;
        let reason = DomainText::required(&input.reason, 4000)?;
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "expected revision must be positive",
            ));
        }
        if let Some(previous) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            Some(&request),
            ctx.actor_id(),
            "local_source.revoke",
        )? {
            let result = PlatformStore::decode(previous)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(result);
        }
        let source = tx.query_opt("SELECT id,owner_actor_id,revision,state FROM local_repository_sources WHERE project_id=$1 AND repository_id=$2 FOR UPDATE", &[&project,&repository])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let id: String = source.get(0);
        let owner: String = source.get(1);
        if source.get::<_, i64>(2) != input.expected_revision {
            return Err(PlatformError::Conflict("local source revision changed"));
        }
        if source.get::<_, String>(3) != "active" {
            return Err(PlatformError::Conflict("local source is already revoked"));
        }
        // Revocation is allowed when the original attesting owner has been suspended.
        tx.query_opt("SELECT id FROM actors WHERE id=$1 FOR SHARE", &[&owner])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        tx.query_opt("SELECT id FROM project_repositories WHERE id=$1 AND project_id=$2 AND source_kind='local_snapshot' FOR UPDATE", &[&repository,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        tx.execute("UPDATE local_repository_sources SET state='revoked',authorization_revision=authorization_revision+1,source_generation=source_generation+1,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1", &[&id]).map_err(PlatformStore::database_error)?;
        let metadata: Value = tx.query_one("UPDATE project_repositories AS r SET authorization_status='revoked',source_generation=source_generation+1,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING repository_metadata(r)->'local_source'", &[&repository]).map_err(PlatformStore::database_error)?.get(0);
        let result: LocalRepositorySourceInfo = PlatformStore::decode(metadata.clone())?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "local_source.revoked",
            Some(repository),
            json!({"project_id":project,"source_id":id,"source_revision":result.source_revision,"authorization_revision":result.authorization_revision,"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, Some(&request), &metadata)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
