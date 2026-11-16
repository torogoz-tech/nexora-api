use crate::{PlatformStore, member_profile_store::MemberProfileStore};
use nexora_platform::{AuthContext, PlatformError};
impl PlatformStore {
    pub fn member_profile(
        &mut self,
        ctx: &AuthContext,
        member: &str,
        year: Option<i32>,
    ) -> Result<serde_json::Value, PlatformError> {
        MemberProfileStore::get(self, ctx, member, year)
    }
}
