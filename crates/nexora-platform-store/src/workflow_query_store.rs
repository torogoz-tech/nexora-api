use crate::{PlatformStore, platform_authority::PlatformAuthority};
use nexora_platform::*;
use serde_json::Value;

pub(crate) struct WorkflowQueryStore;
impl WorkflowQueryStore {
    pub fn versions(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        workflow: &str,
        before: Option<i64>,
        limit: u16,
    ) -> Result<WorkflowVersionPage, PlatformError> {
        if !(1..=100).contains(&limit) || before.is_some_and(|value| value < 1) {
            return Err(PlatformError::Validation("invalid workflow version page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let read_scope = Self::authorize(&mut tx, ctx, project)?;
        if !read_scope.permits(workflow) {
            return Err(PlatformError::NotFound);
        }
        tx.query_opt(
            "SELECT id FROM workflows WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2",
            &[&workflow, &project],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        let rows=tx.query("SELECT to_jsonb(v)-'graph'-'resource_kinds' FROM workflow_versions v WHERE workflow_id=$1 AND project_id IS NOT DISTINCT FROM $2 AND ($3::bigint IS NULL OR number<$3) ORDER BY number DESC LIMIT $4",&[&workflow,&project,&before,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<WorkflowVersionSummary>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_before = if more {
            items.last().map(|item| item.number)
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(WorkflowVersionPage { items, next_before })
    }
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
    ) -> Result<WorkflowDefinition, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if !Self::authorize(&mut tx, ctx, project)?.permits(id) {
            return Err(PlatformError::NotFound);
        }
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(w) FROM workflows w WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2",
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
        project: Option<&str>,
        workflow: &str,
        id: &str,
    ) -> Result<WorkflowVersion, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if !Self::authorize(&mut tx, ctx, project)?.permits(workflow) {
            return Err(PlatformError::NotFound);
        }
        let value:Value=tx.query_opt("SELECT to_jsonb(v) FROM workflow_versions v WHERE id=$1 AND workflow_id=$2 AND project_id IS NOT DISTINCT FROM $3",&[&id,&workflow,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<WorkflowPage, PlatformError> {
        if !(1..=100).contains(&limit) || after.is_some_and(|id| id.is_empty() || id.len() > 128) {
            return Err(PlatformError::Validation("invalid workflow page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let read_scope = Self::authorize(&mut tx, ctx, project)?;
        let own = match &read_scope {
            crate::workflow_read_scope::WorkflowReadScope::All => None,
            crate::workflow_read_scope::WorkflowReadScope::Assigned(id) => Some(id.as_slice()),
            crate::workflow_read_scope::WorkflowReadScope::Unassigned => {
                tx.commit().map_err(PlatformStore::database_error)?;
                return Ok(WorkflowPage {
                    items: vec![],
                    next_after: None,
                });
            }
        };
        let rows=tx.query("SELECT to_jsonb(w)-'draft' FROM workflows w WHERE project_id IS NOT DISTINCT FROM $1 AND ($2 OR NOT archived) AND ($3::text IS NULL OR id>$3) AND ($5::text[] IS NULL OR id=ANY($5)) ORDER BY id LIMIT $4",&[&project,&include_archived,&after,&(i64::from(limit)+1),&own]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<WorkflowSummary>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_after = if more {
            items.last().map(|item| item.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(WorkflowPage { items, next_after })
    }
    fn authorize(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        project: Option<&str>,
    ) -> Result<crate::workflow_read_scope::WorkflowReadScope, PlatformError> {
        if let Some(project) = project {
            PlatformAuthority::require(tx, ctx, Capability::WorkflowsRead, Some(project)).map_err(
                |error| {
                    if matches!(error, PlatformError::Forbidden) {
                        PlatformError::NotFound
                    } else {
                        error
                    }
                },
            )?;
            PlatformAuthority::project(tx, project)?;
            Ok(crate::workflow_read_scope::WorkflowReadScope::All)
        } else {
            crate::workflow_read_authority::WorkflowReadAuthority::scope(tx, ctx).map_err(|error| {
                if matches!(error, PlatformError::Forbidden) {
                    PlatformError::NotFound
                } else {
                    error
                }
            })
        }
    }
}
