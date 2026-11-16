use crate::{PlatformStore, task_release_store::TaskReleaseStore};
use nexora_platform::*;
use serde_json::Value;
impl PlatformStore {
    pub fn task_releases(
        &mut self,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<Value, PlatformError> {
        TaskReleaseStore::list(self, ctx, project)
    }
    pub fn task_release(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<Value, PlatformError> {
        TaskReleaseStore::get(self, ctx, project, id)
    }
    pub fn save_task_release(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: Option<&str>,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        TaskReleaseStore::save(self, ctx, project, id, input)
    }
}
