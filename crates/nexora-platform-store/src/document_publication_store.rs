use crate::{
    PlatformStore, SecretGenerator, document_history_writer::DocumentHistoryWriter,
    document_policy_store::DocumentPolicyStore, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};

pub(crate) struct DocumentPublicationStore;
impl DocumentPublicationStore {
    pub fn publish(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &ProjectDocumentPublish,
    ) -> Result<DocumentPublication, PlatformError> {
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        if input.expected_revision < 1 || input.expected_state_revision < 1 {
            return Err(PlatformError::Validation(
                "expected document revisions must be positive",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsPublish)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "document.publish",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let document = DocumentPolicyStore::definition(&mut tx, project, id, true)?;
        DocumentPolicyStore::active(&document, Some(input.expected_revision))?;
        if document.latest_version_id != input.version_id {
            return Err(PlatformError::Conflict(
                "only latest document revision can be published",
            ));
        }
        let version = DocumentPolicyStore::version(&mut tx, project, id, &input.version_id)?;
        if version.state_revision != input.expected_state_revision {
            return Err(PlatformError::Conflict("document state revision changed"));
        }
        let publication = SecretGenerator::id("dcp")?;
        let value:Value=tx.query_opt("INSERT INTO document_publications AS p(id,project_id,document_id,version_id,checksum,workflow_version_id,state_id,state_revision,published_by,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(version_id,state_revision) DO NOTHING RETURNING to_jsonb(p)",&[&publication,&project,&id,&version.id,&version.checksum,&version.workflow_version_id,&version.state_id,&version.state_revision,&ctx.actor_id(),&reason]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("document state revision already published"))?.get(0);
        tx.execute("UPDATE project_documents SET published_version_id=$3,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2",&[&id,&project,&version.id]).map_err(PlatformStore::database_error)?;
        DocumentHistoryWriter::append(
            &mut tx,
            ctx,
            project,
            id,
            &version.id,
            "document.published",
            &reason,
            json!({"publication_id":publication,"checksum":version.checksum,"state_revision":version.state_revision}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
