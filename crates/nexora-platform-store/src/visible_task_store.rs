use crate::{
    PlatformStore, task_read_authority::TaskReadAuthority,
    task_read_predicate::task_read_predicate, workflow_read_authority::WorkflowReadAuthority,
};
use nexora_platform::*;
pub(crate) struct VisibleTaskStore;
impl VisibleTaskStore {
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        input: &VisibleTaskQuery,
    ) -> Result<VisibleTaskPage, PlatformError> {
        if !(1..=100).contains(&input.limit)
            || [
                &input.after,
                &input.type_code,
                &input.workflow_id,
                &input.project_id,
                &input.assignee_id,
            ]
            .into_iter()
            .flatten()
            .any(|v| v.is_empty() || v.len() > 128)
            || input.search.as_ref().is_some_and(|s| s.len() > 256)
        {
            return Err(PlatformError::Validation("invalid visible task query"));
        }
        let mut tx = store
            .client
            .build_transaction()
            .isolation_level(postgres::IsolationLevel::RepeatableRead)
            .start()
            .map_err(PlatformStore::database_error)?;
        let scope = TaskReadAuthority::scope(&mut tx, ctx)?;
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
        if let Some(workflow) = &input.workflow_id {
            if !WorkflowReadAuthority::scope(&mut tx, ctx)?.permits(workflow) {
                return Err(PlatformError::NotFound);
            }
            tx.query_opt(
                "SELECT id FROM workflows WHERE id=$1 AND project_id IS NULL AND NOT archived",
                &[workflow],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        }
        let visibility = task_read_predicate("$1", "t.project_id=ANY($3)");
        let predicate = format!(
            "( (t.project_id=ANY($2) AND {visibility}) OR (t.project_id IS NULL AND {global}) ) AND NOT t.archived AND ($4::text IS NULL OR t.type_code=$4) AND ($5::text IS NULL OR t.workflow_id=$5) AND ($6::text IS NULL OR ($6='none' AND t.project_id IS NULL) OR t.project_id=$6) AND ($7::text IS NULL OR strpos(lower(t.title),lower($7))>0) AND ($8::text IS NULL OR ($8='none' AND t.assignee_id IS NULL) OR t.assignee_id=$8)"
        );
        let predicate = if let Some(lifecycle) = input.lifecycle {
            format!("{predicate} AND t.lifecycle='{}'", lifecycle.as_str())
        } else {
            predicate
        };
        let predicate =
            if input.workflow_id.is_some() && input.lifecycle == Some(TaskLifecycle::Active) {
                format!(
                    "{predicate} AND {}",
                    crate::task_active_stage_predicate::TASK_ACTIVE_STAGE_PREDICATE
                )
            } else {
                predicate
            };
        let total: i64 = tx
            .query_one(
                &format!("SELECT count(*) FROM tasks t WHERE {predicate}"),
                &[
                    &ctx.actor_id(),
                    &scope.projects,
                    &scope.unrestricted,
                    &input.type_code,
                    &input.workflow_id,
                    &input.project_id,
                    &input.search,
                    &input.assignee_id,
                ],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        let participants = tx
            .query(
                &format!("SELECT t.assignee_id,a.name,count(*) FROM tasks t JOIN actors a ON a.id=t.assignee_id WHERE {predicate} GROUP BY t.assignee_id,a.name ORDER BY a.name,t.assignee_id"),
                &[
                    &ctx.actor_id(),
                    &scope.projects,
                    &scope.unrestricted,
                    &input.type_code,
                    &input.workflow_id,
                    &input.project_id,
                    &input.search,
                    &input.assignee_id,
                ],
            )
            .map_err(PlatformStore::database_error)?
            .into_iter()
            .map(|row| TaskParticipant {
                actor_id: row.get(0),
                name: row.get(1),
                total: row.get(2),
            })
            .collect();
        let direction = if input.descending { "DESC" } else { "ASC" };
        let comparison = if input.descending { "<" } else { ">" };
        let cursor_title = format!(
            "SELECT lower(cursor_task.title) FROM tasks cursor_task WHERE cursor_task.id=$9 AND EXISTS(SELECT 1 FROM tasks t WHERE t.id=cursor_task.id AND {predicate})"
        );
        let sql = format!(
            "SELECT (to_jsonb(t)-'context'-'fields'-'resolution'-'origin_reference'-'template_snapshot')||task_relation_counts(t.id)||jsonb_build_object('project_name',p.name,'assignee_name',a.name,'created_by_name',(SELECT creator.name FROM actors creator WHERE creator.id=t.created_by),'creator',(SELECT jsonb_build_object('id',person.id,'name',person.name,'position',COALESCE((SELECT position FROM member_positions WHERE actor_id=person.id),''),'avatar_revision',COALESCE((SELECT revision FROM user_avatars WHERE actor_id=person.id),0)) FROM actors person WHERE person.id=t.created_by),'assignee',(SELECT jsonb_build_object('id',person.id,'name',person.name,'position',COALESCE((SELECT position FROM member_positions WHERE actor_id=person.id),''),'avatar_revision',COALESCE((SELECT revision FROM user_avatars WHERE actor_id=person.id),0)) FROM actors person WHERE person.id=t.assignee_id),'type_name',COALESCE(tt.name,t.type_code),'type_icon',(SELECT c.icon FROM task_type_catalog c WHERE c.code=t.type_code),'state_changed_at',COALESCE((SELECT h.created_at FROM task_history h WHERE h.task_id=t.id AND jsonb_typeof(h.details->'after')='object' AND (h.details->'before' IS NULL OR h.details->'before'='null'::jsonb OR (h.details->'before'->>'state_id') IS DISTINCT FROM (h.details->'after'->>'state_id') OR (h.details->'before'->>'lifecycle') IS DISTINCT FROM (h.details->'after'->>'lifecycle')) ORDER BY h.sequence DESC LIMIT 1),t.created_at),'state_color',(SELECT s->>'color' FROM jsonb_array_elements(v.graph->'states') s WHERE s->>'id'=t.state_id LIMIT 1),'state_name',COALESCE((SELECT s->>'name' FROM jsonb_array_elements(v.graph->'states') s WHERE s->>'id'=t.state_id LIMIT 1),t.state_id)) FROM tasks t LEFT JOIN projects p ON p.id=t.project_id LEFT JOIN actors a ON a.id=t.assignee_id LEFT JOIN task_type_catalog tt ON tt.code=t.type_code LEFT JOIN workflow_versions v ON v.id=t.workflow_version_id WHERE {predicate} AND ($9::text IS NULL OR (lower(t.title),t.id){comparison}(({cursor_title}),$9::text)) ORDER BY lower(t.title) {direction},t.id {direction} LIMIT $10"
        );
        let mut items = tx
            .query(
                &sql,
                &[
                    &ctx.actor_id(),
                    &scope.projects,
                    &scope.unrestricted,
                    &input.type_code,
                    &input.workflow_id,
                    &input.project_id,
                    &input.search,
                    &input.assignee_id,
                    &input.after,
                    &(i64::from(input.limit) + 1),
                ],
            )
            .map_err(PlatformStore::database_error)?
            .into_iter()
            .map(|row| PlatformStore::decode::<VisibleTask>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(input.limit);
        items.truncate(usize::from(input.limit));
        let next_after = if more {
            items.last().map(|t| t.task.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(VisibleTaskPage {
            items,
            next_after,
            total,
            participants,
            projectless_tasks_included: global
                && input
                    .project_id
                    .as_deref()
                    .is_none_or(|project| project == "none"),
        })
    }
    pub fn types(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
    ) -> Result<VisibleTaskTypePage, PlatformError> {
        if !(1..=100).contains(&limit) || after.is_some_and(|v| v.is_empty() || v.len() > 128) {
            return Err(PlatformError::Validation("invalid visible task type query"));
        }
        let mut tx = store
            .client
            .build_transaction()
            .isolation_level(postgres::IsolationLevel::RepeatableRead)
            .start()
            .map_err(PlatformStore::database_error)?;
        let scope = TaskReadAuthority::scope(&mut tx, ctx)?;
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
        let visibility = task_read_predicate("$1", "t.project_id=ANY($3)");
        let visible = format!(
            "((t.project_id=ANY($2) AND {visibility}) OR (t.project_id IS NULL AND {global})) AND NOT t.archived"
        );
        let total: i64 = tx
            .query_one(
                &format!("SELECT count(*) FROM tasks t WHERE {visible}"),
                &[&ctx.actor_id(), &scope.projects, &scope.unrestricted],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        let sql = format!(
            "WITH counts AS(SELECT t.type_code,count(*) AS total FROM tasks t WHERE {visible} GROUP BY t.type_code) SELECT c.code,c.name,c.color,COALESCE(counts.total,0)::bigint,c.icon FROM task_type_catalog c LEFT JOIN counts ON counts.type_code=c.code WHERE (NOT c.archived OR counts.total>0) AND ({global} OR EXISTS(SELECT 1 FROM task_types tt WHERE tt.code=c.code AND tt.project_id=ANY($2) AND NOT tt.archived) OR counts.total>0) AND ($4::text IS NULL OR c.code>$4) ORDER BY c.code LIMIT $5"
        );
        let mut items = tx
            .query(
                &sql,
                &[
                    &ctx.actor_id(),
                    &scope.projects,
                    &scope.unrestricted,
                    &after,
                    &(i64::from(limit) + 1),
                ],
            )
            .map_err(PlatformStore::database_error)?
            .into_iter()
            .map(|r| {
                Ok(VisibleTaskType {
                    icon: serde_json::from_value(serde_json::Value::String(r.get(4)))
                        .map_err(|_| PlatformError::Storage)?,
                    code: r.get(0),
                    name: r.get(1),
                    color: r.get(2),
                    total: r.get(3),
                })
            })
            .collect::<Result<Vec<_>, PlatformError>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_after = if more {
            items.last().map(|t| t.code.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(VisibleTaskTypePage {
            items,
            next_after,
            total,
        })
    }
}
