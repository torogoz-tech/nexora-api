use crate::{
    PlatformStore, SecretGenerator, document_link_history::DocumentLinkHistory,
    document_link_policy::DocumentLinkPolicy, document_policy_store::DocumentPolicyStore,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::Value;
pub(crate) struct DocumentLinkStore;
impl DocumentLinkStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        document: &str,
        input: &DocumentLinkCreate,
    ) -> Result<DocumentLink, PlatformError> {
        let target_revision = match &input.target {
            DocumentLinkTarget::Task {
                expected_revision, ..
            }
            | DocumentLinkTarget::DocumentVersion {
                expected_revision, ..
            } => *expected_revision,
        };
        if input.expected_document_revision < 1 || target_revision < 1 {
            return Err(PlatformError::Validation(
                "link requires positive resource revisions",
            ));
        }
        if input.kind == DocumentLinkKind::RequiredForTransition
            && !matches!(input.target, DocumentLinkTarget::Task { .. })
        {
            return Err(PlatformError::Validation(
                "document prerequisites only target tasks",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentLinkPolicy::authority(&mut tx, ctx, project, &input.target)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "document.link_create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let scope = DocumentLinkPolicy::lock(&mut tx, project, document, &input.target)?;
        DocumentPolicyStore::active(&scope.source, Some(input.expected_document_revision))?;
        if scope.task.as_ref().is_some_and(|task| task.archived) {
            return Err(PlatformError::Conflict("target task is archived"));
        }
        if let Some(target) = &scope.target_document {
            DocumentPolicyStore::active(target, None)?;
        }
        DocumentPolicyStore::version(&mut tx, project, document, &input.version_id)?;
        let (target_task, target_document, target_version) = match &input.target {
            DocumentLinkTarget::Task { task_id, .. } => (Some(task_id.as_str()), None, None),
            DocumentLinkTarget::DocumentVersion {
                document_id,
                version_id,
                ..
            } => {
                if version_id == &input.version_id {
                    return Err(PlatformError::Validation(
                        "a document version cannot reference itself",
                    ));
                }
                DocumentPolicyStore::version(&mut tx, project, document_id, version_id)?;
                (None, Some(document_id.as_str()), Some(version_id.as_str()))
            }
        };
        if input.kind == DocumentLinkKind::RequiredForTransition {
            let task = target_task.ok_or(PlatformError::Storage)?;
            let count:i64=tx.query_one("SELECT count(*) FROM document_links WHERE project_id=$1 AND target_task_id=$2 AND kind='required_for_transition' AND removed_at IS NULL",&[&project,&task]).map_err(PlatformStore::database_error)?.get(0);
            if count >= 64 {
                return Err(PlatformError::Validation(
                    "task exceeds64 document prerequisites",
                ));
            }
        }
        let id = SecretGenerator::id("dln")?;
        let revisions = DocumentLinkHistory::touch(
            &mut tx,
            ctx,
            &scope,
            &input.version_id,
            target_version,
            &id,
            input.kind,
            false,
            &reason,
        )?;
        let value:Value=tx.query_opt("INSERT INTO document_links AS l(id,project_id,source_document_id,source_version_id,kind,target_task_id,target_document_id,target_version_id,source_revision_at_creation,target_revision_at_creation,reason,created_by,credential_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT DO NOTHING RETURNING to_jsonb(l)",&[&id,&project,&document,&input.version_id,&input.kind.as_str(),&target_task,&target_document,&target_version,&revisions.0,&revisions.1,&reason,&ctx.actor_id(),&ctx.credential_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("active document link already exists"))?.get(0);
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn remove(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        document: &str,
        id: &str,
        input: &DocumentLinkRemove,
    ) -> Result<DocumentLink, PlatformError> {
        if input.expected_document_revision < 1 || input.expected_target_revision < 1 {
            return Err(PlatformError::Validation(
                "link removal requires positive resource revisions",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsWrite)?;
        let current = DocumentLinkPolicy::get(&mut tx, project, document, id)?;
        let target = DocumentLinkPolicy::target(&current, input.expected_target_revision)?;
        DocumentLinkPolicy::authority(&mut tx, ctx, project, &target)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "document.link_remove",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let scope = DocumentLinkPolicy::lock(&mut tx, project, document, &target)?;
        if scope.source.revision != input.expected_document_revision {
            return Err(PlatformError::Conflict("source document revision changed"));
        }
        let current = DocumentLinkPolicy::get(&mut tx, project, document, id)?;
        if current.removed_at.is_some() {
            return Err(PlatformError::Conflict("document link already removed"));
        }
        DocumentLinkHistory::touch(
            &mut tx,
            ctx,
            &scope,
            &current.source_version_id,
            current.target_version_id.as_deref(),
            id,
            current.kind,
            true,
            &reason,
        )?;
        let value:Value=tx.query_one("UPDATE document_links AS l SET removed_at=clock_timestamp(),removed_by=$4,removal_reason=$5 WHERE id=$1 AND project_id=$2 AND source_document_id=$3 RETURNING to_jsonb(l)",&[&id,&project,&document,&ctx.actor_id(),&reason]).map_err(PlatformStore::database_error)?.get(0);
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
