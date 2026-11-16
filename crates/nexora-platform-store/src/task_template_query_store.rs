use crate::{PlatformStore, platform_authority::PlatformAuthority};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::Value;

pub(crate) struct TaskTemplateQueryStore;
impl TaskTemplateQueryStore {
    fn authorize(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<(), PlatformError> {
        PlatformAuthority::require(tx, ctx, Capability::TasksRead, Some(project)).map_err(
            |error| {
                if matches!(error, PlatformError::Forbidden) {
                    PlatformError::NotFound
                } else {
                    error
                }
            },
        )?;
        PlatformAuthority::project(tx, project)
    }
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<TaskTemplate, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authorize(&mut tx, ctx, project)?;
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(t) FROM task_templates t WHERE id=$1 AND project_id=$2",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn version(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        version: &str,
    ) -> Result<TaskTemplateVersion, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authorize(&mut tx, ctx, project)?;
        let value:Value=tx.query_opt("SELECT to_jsonb(v) FROM task_template_versions v WHERE id=$1 AND project_id=$2 AND template_id=$3",&[&version,&project,&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<TaskTemplatePage, PlatformError> {
        if !(1..=100).contains(&limit)
            || after.is_some_and(|value| value.is_empty() || value.len() > 128)
        {
            return Err(PlatformError::Validation("invalid template page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authorize(&mut tx, ctx, project)?;
        let rows=tx.query("SELECT to_jsonb(t) FROM task_templates t WHERE project_id=$1 AND ($2::text IS NULL OR id>$2) AND ($3 OR NOT archived) ORDER BY id LIMIT $4",&[&project,&after,&include_archived,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<TaskTemplate>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_after = if more {
            items.last().map(|item| item.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(TaskTemplatePage { items, next_after })
    }
    pub fn versions(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        before: Option<i64>,
        limit: u16,
    ) -> Result<TaskTemplateVersionPage, PlatformError> {
        if !(1..=100).contains(&limit) || before.is_some_and(|value| value < 1) {
            return Err(PlatformError::Validation("invalid template version page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authorize(&mut tx, ctx, project)?;
        tx.query_opt(
            "SELECT id FROM task_templates WHERE id=$1 AND project_id=$2",
            &[&id, &project],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        let rows=tx.query("SELECT to_jsonb(v)-'definition' FROM task_template_versions v WHERE template_id=$1 AND project_id=$2 AND ($3::bigint IS NULL OR number<$3) ORDER BY number DESC LIMIT $4",&[&id,&project,&before,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<TaskTemplateVersionSummary>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_before = if more {
            items.last().map(|item| item.number)
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(TaskTemplateVersionPage { items, next_before })
    }
}
