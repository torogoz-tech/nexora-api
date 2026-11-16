use crate::{
    PlatformStore, SecretGenerator, document_content_validation::DocumentContentValidation,
    document_history_writer::DocumentHistoryWriter, document_policy_store::DocumentPolicyStore,
    document_revision_store::DocumentRevisionStore, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};

pub(crate) struct ProjectDocumentDefinitionStore;
impl ProjectDocumentDefinitionStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &ProjectDocumentCreate,
    ) -> Result<ProjectDocument, PlatformError> {
        if matches!(input.content, DocumentContent::BestPractice(_)) {
            return Err(PlatformError::Validation(
                "new best practices belong to organization knowledge",
            ));
        }
        let code = DocumentContentValidation::code(&input.code)?;
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
            "document.create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }

        if !input.evidence.is_empty() && store.code_runtime.is_none() {
            store.code_runtime = Some(
                nexora_runtime::DatabaseRuntime::connect(&store.connections)
                    .map_err(|_| PlatformError::Storage)?,
            );
        }
        let verified = crate::document_context_store::DocumentContextStore::capture(
            &mut tx,
            ctx,
            project,
            input.context_manifest_id.as_deref(),
            &input.evidence,
            store.code_runtime.as_mut(),
        )?;
        let id = SecretGenerator::id("doc")?;
        let version = DocumentRevisionStore::id()?;
        let kind = serde_json::to_value(input.content.resource_kind())
            .map_err(|_| PlatformError::Storage)?;
        let kind = kind.as_str().ok_or(PlatformError::Storage)?;
        tx.query_opt("INSERT INTO project_documents(id,project_id,code,kind,latest_version_id,latest_version_number,created_by) VALUES($1,$2,$3,$4,$5,1,$6) ON CONFLICT(project_id,code) DO NOTHING RETURNING id",&[&id,&project,&code,&kind,&version,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("document code already exists"))?;
        let revision = ProjectDocumentRevise {
            expected_revision: 1,
            title: input.title.clone(),
            content: input.content.clone(),
            origin: input.origin,
            reason: reason.clone(),
            context_manifest_id: input.context_manifest_id.clone(),
            evidence: input.evidence.clone(),
        };
        let first = DocumentRevisionStore::insert(
            &mut tx, ctx, project, &id, &version, 1, None, &revision, &verified,
        )?;
        DocumentHistoryWriter::append(
            &mut tx,
            ctx,
            project,
            &id,
            &version,
            "document.created",
            &reason,
            json!({"checksum":first.checksum,"workflow_version_id":first.workflow_version_id,"state_id":first.state_id}),
        )?;
        let result = DocumentPolicyStore::definition(&mut tx, project, &id, false)?;
        ReceiptWriter::save(
            &mut tx,
            request.as_ref(),
            &serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?,
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn revise(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &ProjectDocumentRevise,
    ) -> Result<ProjectDocumentVersion, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "expected document revision must be positive",
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
            "document.revise",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }

        if !input.evidence.is_empty() && store.code_runtime.is_none() {
            store.code_runtime = Some(
                nexora_runtime::DatabaseRuntime::connect(&store.connections)
                    .map_err(|_| PlatformError::Storage)?,
            );
        }
        let verified = crate::document_context_store::DocumentContextStore::capture(
            &mut tx,
            ctx,
            project,
            input.context_manifest_id.as_deref(),
            &input.evidence,
            store.code_runtime.as_mut(),
        )?;
        let current = DocumentPolicyStore::definition(&mut tx, project, id, true)?;
        DocumentPolicyStore::active(&current, Some(input.expected_revision))?;
        if input.content.resource_kind() != current.kind {
            return Err(PlatformError::Validation("document kind cannot change"));
        }
        let number = DocumentRevisionStore::next_number(&current)?;
        let version = DocumentRevisionStore::id()?;
        let result = DocumentRevisionStore::insert(
            &mut tx,
            ctx,
            project,
            id,
            &version,
            number,
            Some(&current.latest_version_id),
            input,
            &verified,
        )?;
        tx.execute("UPDATE project_documents SET latest_version_id=$3,latest_version_number=$4,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2",&[&id,&project,&version,&number]).map_err(PlatformStore::database_error)?;
        DocumentHistoryWriter::append(
            &mut tx,
            ctx,
            project,
            id,
            &version,
            "document.revised",
            &reason,
            json!({"previous_version_id":current.latest_version_id,"checksum":result.checksum,"number":number}),
        )?;
        ReceiptWriter::save(
            &mut tx,
            request.as_ref(),
            &serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?,
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn change(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &ProjectDocumentChange,
    ) -> Result<ProjectDocument, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "expected document revision must be positive",
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
            "document.change",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let current = DocumentPolicyStore::definition(&mut tx, project, id, true)?;
        if current.revision != input.expected_revision {
            return Err(PlatformError::Conflict("document revision changed"));
        }
        let value:Value=tx.query_one("UPDATE project_documents d SET archived=$3,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2 RETURNING to_jsonb(d)",&[&id,&project,&input.archived]).map_err(PlatformStore::database_error)?.get(0);
        DocumentHistoryWriter::append(
            &mut tx,
            ctx,
            project,
            id,
            &current.latest_version_id,
            "document.changed",
            &reason,
            json!({"archived":input.archived,"revision":value["revision"]}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
