use crate::{
    PlatformStore, document_context_store::DocumentContextStore,
    document_history_writer::DocumentHistoryWriter, document_policy_store::DocumentPolicyStore,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct DocumentApprovalRevocationStore;
impl DocumentApprovalRevocationStore {
    pub fn revoke(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        document: &str,
        version: &str,
        id: &str,
        input: &DocumentApprovalRevoke,
    ) -> Result<DocumentApproval, PlatformError> {
        if ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::ApprovalsGrant, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "document.approval_revoke",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let source = DocumentPolicyStore::version(&mut tx, project, document, version)?;
        DocumentContextStore::lock_version(&mut tx, &source)?;
        DocumentPolicyStore::definition(&mut tx, project, document, true)?;
        let value:Value=tx.query_opt("SELECT to_jsonb(a) FROM document_approvals a WHERE id=$1 AND project_id=$2 AND document_id=$3 AND version_id=$4 FOR UPDATE",&[&id,&project,&document,&version]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        let before: DocumentApproval = PlatformStore::decode(value)?;
        if before.consumed_at.is_some() || before.revoked_at.is_some() {
            return Err(PlatformError::Conflict(
                "document approval already consumed or revoked",
            ));
        }
        let value:Value=tx.query_one("UPDATE document_approvals AS a SET revoked_at=clock_timestamp(),revoked_by=$2,revocation_reason=$3 WHERE id=$1 RETURNING to_jsonb(a)",&[&id,&ctx.actor_id(),&reason]).map_err(PlatformStore::database_error)?.get(0);
        DocumentHistoryWriter::append(
            &mut tx,
            ctx,
            project,
            document,
            version,
            "document.approval_revoked",
            &reason,
            json!({"approval_id":id}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
