use crate::PlatformStore;
use nexora_platform::*;
impl PlatformStore {
    pub fn visible_tasks(
        &mut self,
        ctx: &AuthContext,
        input: &VisibleTaskQuery,
    ) -> Result<VisibleTaskPage, PlatformError> {
        crate::visible_task_store::VisibleTaskStore::page(self, ctx, input)
    }
    pub fn visible_task_types(
        &mut self,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
    ) -> Result<VisibleTaskTypePage, PlatformError> {
        crate::visible_task_store::VisibleTaskStore::types(self, ctx, after, limit)
    }
}
