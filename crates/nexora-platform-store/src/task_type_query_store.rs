use crate::{
    PlatformStore, document_page_validation::DocumentPageValidation,
    platform_authority::PlatformAuthority,
};
use nexora_platform::*;
pub(crate) struct TaskTypeQueryStore;
impl TaskTypeQueryStore {
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<TaskTypePage, PlatformError> {
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
        let rows=tx.query("SELECT to_jsonb(c)||jsonb_build_object('project_id',$1::text) FROM task_type_catalog c WHERE ($2::text IS NULL OR code>$2) AND ($3 OR NOT archived) ORDER BY code LIMIT $4",&[&project,&after,&include_archived,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<TaskType>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_after = if more {
            items.last().map(|item| item.code.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(TaskTypePage { items, next_after })
    }
}
