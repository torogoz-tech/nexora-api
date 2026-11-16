use crate::PlatformStore;
use nexora_platform::*;

pub(crate) struct TaskDependencyQueryStore;
impl TaskDependencyQueryStore {
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        task: &str,
        input: &TaskDependencyQuery,
    ) -> Result<TaskDependencyPage, PlatformError> {
        if !(1..=100).contains(&input.limit)
            || input
                .after
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > 128)
        {
            return Err(PlatformError::Validation("invalid dependency page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let before = crate::task_scope_authority::TaskScopeAuthority::task(
            &mut tx,
            ctx,
            task,
            Capability::TasksRead,
        )?;
        if project.is_some() && project != before.project_id.as_deref() {
            return Err(PlatformError::NotFound);
        }
        let project = before.project_id.as_deref();
        let kind = input.kind.map(TaskDependencyKind::as_str);
        let status = input.status.map(TaskDependencyStatus::as_str);
        let rows=tx.query("SELECT to_jsonb(d) FROM task_dependencies d WHERE project_id IS NOT DISTINCT FROM $1 AND (source_task_id=$2 OR target_task_id=$2) AND ($3::text IS NULL OR id>$3) AND ($4 OR removed_at IS NULL) AND ($5::text IS NULL OR kind=$5) AND ($6::text IS NULL OR status=$6) ORDER BY id LIMIT $7",&[&project,&task,&input.after,&input.include_removed,&kind,&status,&(i64::from(input.limit)+1)]).map_err(PlatformStore::database_error)?;
        let more = rows.len() > usize::from(input.limit);
        let next_after = if more {
            rows.get(usize::from(input.limit) - 1).map(|row| {
                row.get::<_, serde_json::Value>(0)["id"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
        } else {
            None
        };
        let mut items = Vec::new();
        for row in rows.into_iter().take(usize::from(input.limit)) {
            let dependency: TaskDependency = PlatformStore::decode(row.get(0))?;
            let mut visible = true;
            for id in [&dependency.source_task_id, &dependency.target_task_id] {
                match crate::task_scope_authority::TaskScopeAuthority::task(
                    &mut tx,
                    ctx,
                    id,
                    Capability::TasksRead,
                ) {
                    Ok(_) => {}
                    Err(PlatformError::Forbidden | PlatformError::NotFound) => visible = false,
                    Err(e) => return Err(e),
                }
            }
            if visible {
                items.push(dependency);
            }
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(TaskDependencyPage { items, next_after })
    }
}
