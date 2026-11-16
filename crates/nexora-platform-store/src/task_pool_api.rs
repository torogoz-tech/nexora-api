use crate::{PlatformStore, task_pool_store::TaskPoolStore};
use nexora_platform::*;
impl PlatformStore {
    pub fn create_pool_task(
        &mut self,
        ctx: &AuthContext,
        input: &PoolTaskCreate,
    ) -> Result<WorkTask, PlatformError> {
        TaskPoolStore::create(self, ctx, input.project_id.as_deref(), &input.definition)
    }
    pub fn take_task(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &TaskTake,
        project: Option<&str>,
    ) -> Result<WorkTask, PlatformError> {
        TaskPoolStore::take(self, ctx, id, input, project)
    }
    pub fn pool_task(&mut self, ctx: &AuthContext, id: &str) -> Result<WorkTask, PlatformError> {
        let scope = self.pool_task_scope(ctx, id, Capability::TasksRead)?;
        crate::task_query_store::TaskQueryStore::get(self, ctx, scope.as_deref(), id)
    }
    pub fn pool_task_scope(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        capability: Capability,
    ) -> Result<Option<String>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let task =
            crate::task_scope_authority::TaskScopeAuthority::task(&mut tx, ctx, id, capability)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(task.project_id)
    }
    pub fn change_pool_task(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &TaskChange,
    ) -> Result<WorkTask, PlatformError> {
        let scope = self.pool_task_scope(ctx, id, Capability::TasksWrite)?;
        crate::task_definition_store::TaskDefinitionStore::change(
            self,
            ctx,
            scope.as_deref(),
            id,
            input,
        )
    }
    pub fn transition_pool_task(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        transition: &str,
        input: &TaskTransitionRequest,
    ) -> Result<WorkTask, PlatformError> {
        let scope = self.pool_task_scope(ctx, id, Capability::TasksTransition)?;
        crate::task_transition_store::TaskTransitionStore::transition(
            self,
            ctx,
            scope.as_deref(),
            id,
            transition,
            input,
        )
    }
    pub fn pool_task_history(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        after: Option<i64>,
        limit: u16,
    ) -> Result<TaskHistoryPage, PlatformError> {
        let scope = self.pool_task_scope(ctx, id, Capability::TasksRead)?;
        crate::task_query_store::TaskQueryStore::history(
            self,
            ctx,
            scope.as_deref(),
            id,
            after,
            limit,
        )
    }
    pub fn pool_task_comments(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        after: Option<&str>,
        limit: u16,
    ) -> Result<TaskCommentPage, PlatformError> {
        let scope = self.pool_task_scope(ctx, id, Capability::TasksRead)?;
        crate::task_comment_query_store::TaskCommentQueryStore::page(
            self,
            ctx,
            scope.as_deref(),
            id,
            after,
            limit,
        )
    }
    pub fn add_pool_task_comment(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &TaskCommentCreate,
    ) -> Result<TaskComment, PlatformError> {
        let scope = self.pool_task_scope(ctx, id, Capability::TasksWrite)?;
        crate::task_comment_store::TaskCommentStore::add(self, ctx, scope.as_deref(), id, input)
    }
    pub fn verify_task_project(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        project: Option<&str>,
    ) -> Result<(), PlatformError> {
        let scope = self.pool_task_scope(ctx, id, Capability::TasksRead)?;
        if project.is_some() && project != scope.as_deref() {
            return Err(PlatformError::NotFound);
        }
        Ok(())
    }
    pub fn grant_pool_task_approval(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &TaskApprovalGrant,
    ) -> Result<TaskApproval, PlatformError> {
        let scope = self.pool_task_scope(ctx, id, Capability::TasksRead)?;
        crate::task_approval_store::TaskApprovalStore::grant(self, ctx, scope.as_deref(), id, input)
    }
    pub fn get_pool_task_approval(
        &mut self,
        ctx: &AuthContext,
        task: &str,
        id: &str,
    ) -> Result<TaskApproval, PlatformError> {
        let scope = self.pool_task_scope(ctx, task, Capability::TasksRead)?;
        crate::task_approval_query_store::TaskApprovalQueryStore::get(
            self,
            ctx,
            scope.as_deref(),
            task,
            id,
        )
    }
    pub fn revoke_pool_task_approval(
        &mut self,
        ctx: &AuthContext,
        task: &str,
        id: &str,
        input: &TaskApprovalRevoke,
    ) -> Result<TaskApproval, PlatformError> {
        let scope = self.pool_task_scope(ctx, task, Capability::TasksRead)?;
        crate::task_approval_revocation_store::TaskApprovalRevocationStore::revoke(
            self,
            ctx,
            scope.as_deref(),
            task,
            id,
            input,
        )
    }
}
