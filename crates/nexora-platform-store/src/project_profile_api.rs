use crate::{PlatformStore, project_profile_store::ProjectProfileStore};
use nexora_platform::{AuthContext, PlatformError};
impl PlatformStore {
    pub fn project_profile(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        year: Option<i32>,
    ) -> Result<serde_json::Value, PlatformError> {
        ProjectProfileStore::get(self, ctx, project, year)
    }
}
