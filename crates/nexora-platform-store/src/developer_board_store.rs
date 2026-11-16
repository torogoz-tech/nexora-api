use crate::{PlatformStore, platform_authority::PlatformAuthority};
use nexora_platform::*;
pub(crate) struct DeveloperBoardStore;
impl DeveloperBoardStore {
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
    ) -> Result<DeveloperBoardPage, PlatformError> {
        if !(1..=100).contains(&limit) || after.is_some_and(|id| id.is_empty() || id.len() > 128) {
            return Err(PlatformError::Validation("invalid developer task page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::workflow_read_authority::WorkflowReadAuthority::scope(&mut tx, ctx)?;
        let candidates=tx.query("SELECT id FROM projects p WHERE NOT archived AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='tasks:read' AND (g.project_id IS NULL OR g.project_id=p.id)) ORDER BY id",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        let mut projects = Vec::<String>::new();
        for row in candidates {
            let id: String = row.get(0);
            match PlatformAuthority::require(&mut tx, ctx, Capability::TasksRead, Some(&id)) {
                Ok(()) => projects.push(id),
                Err(PlatformError::Forbidden) => {}
                Err(error) => return Err(error),
            }
        }
        let global = match crate::task_scope_authority::TaskScopeAuthority::require(
            &mut tx,
            ctx,
            Capability::TasksRead,
            None,
        ) {
            Ok(()) => true,
            Err(PlatformError::Forbidden) => false,
            Err(error) => return Err(error),
        };
        let rows=tx.query(&format!("SELECT (to_jsonb(t)-'context'-'fields'-'resolution'-'origin_reference')||task_relation_counts(t.id)||jsonb_build_object('project_name',p.name) FROM tasks t JOIN developer_workflows d ON d.actor_id=t.assignee_id AND d.workflow_id=t.workflow_id LEFT JOIN projects p ON p.id=t.project_id WHERE t.assignee_id=$1 AND (t.project_id=ANY($2) OR (t.project_id IS NULL AND $5)) AND t.lifecycle='active' AND NOT t.archived AND {} AND ($3::text IS NULL OR t.id>$3) ORDER BY t.id LIMIT $4",crate::task_active_stage_predicate::TASK_ACTIVE_STAGE_PREDICATE),&[&ctx.actor_id(),&projects,&after,&(i64::from(limit)+1),&global]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<DeveloperBoardTask>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_after = if more {
            items.last().map(|item| item.task.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(DeveloperBoardPage { items, next_after })
    }
}
