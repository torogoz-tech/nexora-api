use crate::{OidcNetworkPolicy, PlatformStore};
use nexora_platform::{EmailAccessSettings, PlatformError};
impl PlatformStore {
    /// The administrator-managed public URL is the source of truth for OIDC returns.
    /// Provider network destinations remain restricted separately from browser redirects.
    pub fn installation_oidc_policy(
        &mut self,
        base: &OidcNetworkPolicy,
    ) -> Result<OidcNetworkPolicy, PlatformError> {
        let public: String = self
            .client
            .query_one(
                "SELECT public_web_url FROM email_access_settings WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        let mut policy = base.clone();
        if !public.is_empty() {
            policy.allowed_callback_origins = vec![EmailAccessSettings::validate(&public)?];
        }
        policy.validate()?;
        Ok(policy)
    }
}
