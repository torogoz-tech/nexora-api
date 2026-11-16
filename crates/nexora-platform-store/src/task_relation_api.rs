use crate::PlatformStore;
use nexora_platform::*;
use serde_json::Value;
impl PlatformStore {
    pub fn change_pool_dependency(
        &mut self,
        ctx: &AuthContext,
        task: Option<&str>,
        project: Option<&str>,
        id: &str,
        input: &TaskDependencyChange,
        remove: bool,
    ) -> Result<TaskDependency, PlatformError> {
        crate::task_relation_dependency_store::TaskRelationDependencyStore::change(
            self, ctx, task, project, id, input, remove,
        )
    }
    pub fn pool_task_dependency_page(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
        task: &str,
        input: &TaskDependencyQuery,
    ) -> Result<TaskDependencyPage, PlatformError> {
        crate::task_dependency_query_store::TaskDependencyQueryStore::page(
            self, ctx, project, task, input,
        )
    }

    pub fn task_relations(&mut self, ctx: &AuthContext, id: &str) -> Result<Value, PlatformError> {
        crate::task_relation_query_store::TaskRelationQueryStore::get(self, ctx, id)
    }
    pub fn create_task_subtask(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &TaskSubtaskCreate,
    ) -> Result<WorkTask, PlatformError> {
        crate::task_subtask_store::TaskSubtaskStore::create(self, ctx, id, input)
    }
    pub fn change_task_subtask(
        &mut self,
        ctx: &AuthContext,
        parent: &str,
        child: &str,
        input: &TaskSubtaskChange,
        remove: bool,
    ) -> Result<Value, PlatformError> {
        crate::task_subtask_store::TaskSubtaskStore::change(self, ctx, parent, child, input, remove)
    }
    pub fn create_pool_task_dependency(
        &mut self,
        ctx: &AuthContext,
        task: &str,
        input: &TaskDependencyCreate,
    ) -> Result<TaskDependency, PlatformError> {
        if task != input.source_task_id {
            return Err(PlatformError::Validation(
                "dependency source must match task",
            ));
        }
        crate::task_relation_dependency_store::TaskRelationDependencyStore::create(
            self, ctx, input, None,
        )
    }
    pub fn remove_pool_task_dependency(
        &mut self,
        ctx: &AuthContext,
        task: &str,
        id: &str,
        input: &TaskDependencyChange,
    ) -> Result<TaskDependency, PlatformError> {
        crate::task_relation_dependency_store::TaskRelationDependencyStore::change(
            self,
            ctx,
            Some(task),
            None,
            id,
            input,
            true,
        )
    }
}
