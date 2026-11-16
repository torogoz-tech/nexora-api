use crate::{PlatformStore, authentication_store::AuthenticationStore};
use nexora_platform::*;
pub(crate) struct GitInitiator;
impl GitInitiator {
    pub fn load(
        store: &mut PlatformStore,
        actor: &str,
        method: AuthMethod,
        credential: &str,
    ) -> Result<AuthContext, PlatformError> {
        let ctx = AuthenticationStore::new(store).load(method, credential)?;
        if ctx.actor_id() != actor {
            return Err(PlatformError::Unauthorized);
        }
        ctx.require(Capability::ConnectionsManage, None)?;
        Ok(ctx)
    }
}
