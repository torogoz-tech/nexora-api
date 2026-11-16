use crate::{PlatformStore, document_policy_store::DocumentPolicyStore};
use nexora_platform::*;

pub(crate) struct DocumentQueryStore;
impl DocumentQueryStore {
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ProjectDocument, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
        let result = DocumentPolicyStore::definition(&mut tx, project, id, false)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn version(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        version: &str,
    ) -> Result<ProjectDocumentVersion, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
        let result = DocumentPolicyStore::version(&mut tx, project, id, version)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn versions(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        before: Option<i64>,
        limit: u16,
    ) -> Result<DocumentVersionPage, PlatformError> {
        if !(1..=100).contains(&limit) || before.is_some_and(|value| value < 1) {
            return Err(PlatformError::Validation("invalid document version page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
        DocumentPolicyStore::definition(&mut tx, project, id, false)?;
        let rows=tx.query("SELECT jsonb_build_object('context_manifest_id',v.context_manifest_id,'needs_review',CASE WHEN v.context_manifest_id IS NULL THEN false ELSE (project_context_status(v.project_id,v.context_manifest_id)->>'needs_review')::boolean END,'id',v.id,'number',v.number,'title',v.title,'origin',v.origin,'workflow_version_id',v.workflow_version_id,'state_id',s.state_id,'state_revision',s.state_revision,'checksum',v.checksum,'created_by',v.created_by,'created_at',v.created_at) FROM project_document_versions v JOIN document_version_states s ON s.version_id=v.id WHERE v.document_id=$1 AND v.project_id=$2 AND ($3::bigint IS NULL OR v.number<$3) ORDER BY v.number DESC LIMIT $4",&[&id,&project,&before,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<DocumentVersionSummary>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_before = if more {
            items.last().map(|item| item.number)
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(DocumentVersionPage { items, next_before })
    }
}
