use crate::PlatformStore;
use nexora_platform::*;
pub(crate) struct TaskDependencyLifecycleStore;
impl TaskDependencyLifecycleStore {
    pub fn resolve(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskDependencyChange,
    ) -> Result<TaskDependency, PlatformError> {
        crate::task_relation_dependency_store::TaskRelationDependencyStore::change(
            store,
            ctx,
            None,
            Some(project),
            id,
            input,
            false,
        )
    }
    pub fn remove(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskDependencyChange,
    ) -> Result<TaskDependency, PlatformError> {
        crate::task_relation_dependency_store::TaskRelationDependencyStore::change(
            store,
            ctx,
            None,
            Some(project),
            id,
            input,
            true,
        )
    }
}
