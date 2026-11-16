use crate::PlatformStore;
use nexora_platform::*;
impl PlatformStore {
    pub fn task_stages(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Vec<TaskStageCompletion>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        crate::task_scope_authority::TaskScopeAuthority::task(
            &mut tx,
            ctx,
            id,
            Capability::TasksRead,
        )?;
        let rows=tx.query("SELECT to_jsonb(s) FROM task_stage_completions s WHERE task_id=$1 ORDER BY completed_at,task_revision",&[&id]).map_err(Self::database_error)?;
        let result = rows
            .into_iter()
            .map(|r| Self::decode(r.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
}
