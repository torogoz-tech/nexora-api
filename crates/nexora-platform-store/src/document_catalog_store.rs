use crate::{
    PlatformStore, document_page_validation::DocumentPageValidation,
    document_policy_store::DocumentPolicyStore,
};
use nexora_platform::*;

pub(crate) struct DocumentCatalogStore;
impl DocumentCatalogStore {
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &DocumentListQuery,
    ) -> Result<DocumentPage, PlatformError> {
        DocumentPageValidation::limit(input.limit)?;
        DocumentPageValidation::cursor(input.after.as_deref())?;
        if input.kind == Some(WorkflowResourceKind::Task) {
            return Err(PlatformError::Validation("task is not a document kind"));
        }
        let kind = input
            .kind
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| PlatformError::Storage)?;
        let kind = kind.as_ref().and_then(|value| value.as_str());
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
        let rows=tx.query("SELECT to_jsonb(d)||jsonb_build_object('context_manifest_id',v.context_manifest_id,'needs_review',CASE WHEN v.context_manifest_id IS NULL THEN false ELSE (project_context_status(v.project_id,v.context_manifest_id)->>'needs_review')::boolean END,'latest_title',v.title,'latest_checksum',v.checksum,'workflow_version_id',v.workflow_version_id,'state_id',s.state_id,'state_revision',s.state_revision) FROM project_documents d JOIN project_document_versions v ON v.id=d.latest_version_id AND v.document_id=d.id AND v.project_id=d.project_id JOIN document_version_states s ON s.version_id=v.id WHERE d.project_id=$1 AND ($2::text IS NULL OR d.id>$2) AND ($3::text IS NULL OR d.kind=$3) AND ($4 OR NOT d.archived) ORDER BY d.id LIMIT $5",&[&project,&input.after,&kind,&input.include_archived,&(i64::from(input.limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<DocumentSummary>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(input.limit);
        items.truncate(usize::from(input.limit));
        let next_after = if more {
            items.last().map(|item| item.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(DocumentPage { items, next_after })
    }
}
