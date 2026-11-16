use crate::{PlatformStore, task_scope_authority::TaskScopeAuthority};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn task_pool_options(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        TaskScopeAuthority::require(&mut tx, ctx, Capability::TasksWrite, project)?;
        let types:Vec<Value>=tx.query("SELECT jsonb_build_object('code',c.code,'name',c.name,'icon',c.icon,'color',c.color,'purpose',c.purpose) FROM task_type_catalog c WHERE NOT c.archived ORDER BY c.name,c.code",&[]).map_err(Self::database_error)?.into_iter().map(|row|row.get(0)).collect();
        let assignees:Vec<Value>=tx.query("SELECT jsonb_build_object('actor_id',a.id,'name',a.name) FROM actors a JOIN actor_roles r ON r.actor_id=a.id WHERE a.active AND a.kind='human' AND r.role IN ('developer','technical_lead','pmo','manager','super_admin') AND ($1::text IS NULL OR EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=a.id AND g.capability='tasks:read' AND (g.project_id IS NULL OR g.project_id=$1))) ORDER BY a.name,a.id",&[&project]).map_err(Self::database_error)?.into_iter().map(|row|row.get(0)).collect();
        let workflows: Vec<Value> = tx.query("SELECT jsonb_build_object('id',w.id,'name',w.name,'assignee_ids',COALESCE((SELECT jsonb_agg(d.actor_id) FROM developer_workflows d WHERE d.workflow_id=w.id),'[]'::jsonb)) FROM workflows w WHERE NOT w.archived AND w.latest_version_id IS NOT NULL AND w.resource_kinds @> '[\"task\"]'::jsonb ORDER BY w.name,w.id", &[]).map_err(Self::database_error)?.into_iter().map(|row| row.get(0)).collect();
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"types":types,"assignees":assignees,"workflows":workflows}))
    }
}
