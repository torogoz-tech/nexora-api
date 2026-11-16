use crate::PlatformStore;
use nexora_platform::*;
pub(crate) struct TaskDependencyCreationStore;
impl TaskDependencyCreationStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &TaskDependencyCreate,
    ) -> Result<TaskDependency, PlatformError> {
        crate::task_relation_dependency_store::TaskRelationDependencyStore::create(
            store,
            ctx,
            input,
            Some(project),
        )
    }
}
