use crate::{PlatformStore, task_scope_authority::TaskScopeAuthority};
use nexora_platform::{AuthContext, Capability, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn task_workflow_timeline(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        after: Option<i64>,
    ) -> Result<Value, PlatformError> {
        if after.is_some_and(|value| value < 0) {
            return Err(PlatformError::Validation("invalid task timeline cursor"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let task = TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksRead)?;
        let mut items:Vec<Value>=tx.query("SELECT jsonb_build_object('id',h.id,'sequence',h.sequence,'revision',h.revision,'operation',h.operation,'actor_id',h.actor_id,'created_at',h.created_at,'details',CASE WHEN operation IN ('task.transition_failed','task.transition_waiting_approval') THEN details ELSE jsonb_build_object('before',jsonb_build_object('state_id',details->'before'->'state_id','workflow_version_id',details->'before'->'workflow_version_id'),'after',jsonb_build_object('state_id',details->'after'->'state_id','workflow_id',details->'after'->'workflow_id','workflow_version_id',details->'after'->'workflow_version_id')) END) FROM task_history h WHERE task_id=$1 AND project_id IS NOT DISTINCT FROM $2 AND ($3::bigint IS NULL OR sequence>$3) AND (operation IN ('task.transition_failed','task.transition_waiting_approval','task.pause','task.resume') OR (details->'after'->>'state_id' IS NOT NULL AND (operation='task.taken' OR details->'before'->>'state_id' IS DISTINCT FROM details->'after'->>'state_id' OR details->'before'->>'workflow_version_id' IS DISTINCT FROM details->'after'->>'workflow_version_id'))) ORDER BY sequence LIMIT 101",&[&id,&task.project_id,&after]).map_err(Self::database_error)?.into_iter().map(|row|row.get(0)).collect();
        let more = items.len() > 100;
        items.truncate(100);
        let next = if more {
            items.last().and_then(|item| item["sequence"].as_i64())
        } else {
            None
        };
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":items,"next_after":next}))
    }
}
