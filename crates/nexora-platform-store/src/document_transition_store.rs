use crate::{
    PlatformStore, document_history_writer::DocumentHistoryWriter,
    document_policy_store::DocumentPolicyStore, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::json;

pub(crate) struct DocumentTransitionStore;
impl DocumentTransitionStore {
    pub fn transition(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        version: &str,
        edge: &str,
        input: &DocumentTransitionRequest,
    ) -> Result<ProjectDocumentVersion, PlatformError> {
        if input.expected_state_revision < 1 {
            return Err(PlatformError::Validation(
                "expected document state revision must be positive",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsWrite)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "document.transition",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let source_version = DocumentPolicyStore::version(&mut tx, project, id, version)?;
        crate::document_context_store::DocumentContextStore::lock_version(
            &mut tx,
            &source_version,
        )?;
        let document = DocumentPolicyStore::definition(&mut tx, project, id, true)?;
        DocumentPolicyStore::active(&document, None)?;
        let before = DocumentPolicyStore::version(&mut tx, project, id, version)?;
        if before.state_revision != input.expected_state_revision {
            return Err(PlatformError::Conflict("document state revision changed"));
        }
        let flow = DocumentPolicyStore::flow(
            &mut tx,
            project,
            before
                .workflow_version_id
                .as_deref()
                .ok_or(PlatformError::Validation(
                    "knowledge does not use workflow transitions",
                ))?,
            document.kind,
            false,
        )?;
        let mut context =
            crate::document_transition_evaluation::DocumentTransitionEvaluation::build(
                &document, &before, input,
            )?;
        context.human_approval_valid =
            crate::document_approval_store::DocumentApprovalStore::consume(
                &mut tx, ctx, &document, &before, &flow, edge, input,
            )?;
        let transition = flow.graph.transition(&before.state_id, edge, &context)?;
        tx.execute("UPDATE document_version_states SET state_id=$2,state_revision=state_revision+1,updated_at=clock_timestamp() WHERE version_id=$1",&[&version,&transition.to]).map_err(PlatformStore::database_error)?;
        DocumentHistoryWriter::append(
            &mut tx,
            ctx,
            project,
            id,
            version,
            "document.transitioned",
            &reason,
            json!({"transition_id":edge,"from":before.state_id,"to":transition.to,"previous_state_revision":before.state_revision}),
        )?;
        let result = DocumentPolicyStore::version(&mut tx, project, id, version)?;
        ReceiptWriter::save(
            &mut tx,
            request.as_ref(),
            &serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?,
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
