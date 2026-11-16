use crate::PlatformStore;
use nexora_platform::{AuthContext, Capability, PlatformError};
impl PlatformStore {
    pub fn can_disable_local_login(&mut self, ctx: &AuthContext) -> Result<bool, PlatformError> {
        let ctx = self.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
        self.client.query_one("SELECT EXISTS(SELECT 1 FROM sessions s JOIN actors a ON a.id=s.actor_id JOIN oidc_providers p ON p.id=s.authentication_provider_id JOIN actor_grants g ON g.actor_id=a.id WHERE a.active AND p.enabled AND g.capability='installation:manage' AND g.project_id IS NULL AND s.access_stage='ready' AND s.revoked_at IS NULL AND s.expires_at>now())",&[]).map(|r|r.get(0)).map_err(Self::database_error)
    }
}
