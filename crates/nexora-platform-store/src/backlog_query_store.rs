use crate::{
    PlatformStore, document_page_validation::DocumentPageValidation,
    platform_authority::PlatformAuthority,
};
use nexora_platform::*;
pub(crate) struct BacklogQueryStore;
impl BacklogQueryStore {
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<BacklogPage, PlatformError> {
        DocumentPageValidation::limit(limit)?;
        DocumentPageValidation::cursor(after)?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::TasksRead, Some(project)).map_err(
            |error| {
                if matches!(error, PlatformError::Forbidden) {
                    PlatformError::NotFound
                } else {
                    error
                }
            },
        )?;
        PlatformAuthority::project(&mut tx, project)?;
        let rows=tx.query("SELECT to_jsonb(t) FROM backlogs t WHERE project_id=$1 AND ($2::text IS NULL OR id>$2) AND ($3 OR NOT archived) ORDER BY id LIMIT $4",&[&project,&after,&include_archived,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<Backlog>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_after = if more {
            items.last().map(|item| item.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(BacklogPage { items, next_after })
    }
}
