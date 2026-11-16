use crate::{
    PlatformStore, document_page_validation::DocumentPageValidation,
    document_policy_store::DocumentPolicyStore, platform_authority::PlatformAuthority,
};
use nexora_platform::*;
pub(crate) struct DocumentLinkQueryStore;
impl DocumentLinkQueryStore {
    #[allow(clippy::too_many_arguments)]
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        task: bool,
        after: Option<&str>,
        limit: u16,
        include_removed: bool,
    ) -> Result<DocumentLinkPage, PlatformError> {
        DocumentPageValidation::limit(limit)?;
        DocumentPageValidation::cursor(after)?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if task {
            PlatformAuthority::require(&mut tx, ctx, Capability::TasksRead, Some(project))
                .map_err(|error| {
                    if matches!(error, PlatformError::Forbidden) {
                        PlatformError::NotFound
                    } else {
                        error
                    }
                })?;
            PlatformAuthority::project(&mut tx, project)?;
            crate::task_read_authority::TaskReadAuthority::require_task(&mut tx, ctx, project, id)?;
            tx.query_opt(
                "SELECT id FROM tasks WHERE id=$1 AND project_id=$2",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        } else {
            DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
            DocumentPolicyStore::definition(&mut tx, project, id, false)?;
        }
        let rows=tx.query("SELECT to_jsonb(l) FROM document_links l WHERE project_id=$1 AND (($3 AND target_task_id=$2) OR (NOT $3 AND (source_document_id=$2 OR target_document_id=$2))) AND ($4::text IS NULL OR l.id>$4) AND ($5 OR removed_at IS NULL) ORDER BY l.id LIMIT $6",&[&project,&id,&task,&after,&include_removed,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<DocumentLink>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_after = if more {
            items.last().map(|item| item.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(DocumentLinkPage { items, next_after })
    }
}
