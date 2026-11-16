use nexora_platform::{OidcProviderRegistration, PlatformError};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OidcNetworkPolicy {
    pub allowed_provider_origins: Vec<String>,
    pub allowed_callback_origins: Vec<String>,
}
impl Default for OidcNetworkPolicy {
    fn default() -> Self {
        Self {
            allowed_provider_origins: vec![
                "https://accounts.google.com".into(),
                "https://oauth2.googleapis.com".into(),
                "https://www.googleapis.com".into(),
                "https://login.microsoftonline.com".into(),
            ],
            allowed_callback_origins: vec!["http://127.0.0.1:18080".into()],
        }
    }
}
impl OidcNetworkPolicy {
    pub fn load(path: &Path) -> Result<Self, PlatformError> {
        let data = std::fs::read(path).map_err(|_| PlatformError::Storage)?;
        if data.len() > 65536 {
            return Err(PlatformError::Validation("identity policy exceeds limit"));
        }
        let policy: Self = serde_json::from_slice(&data)
            .map_err(|_| PlatformError::Validation("invalid identity policy"))?;
        policy.validate()?;
        Ok(policy)
    }
    pub fn validate(&self) -> Result<(), PlatformError> {
        for list in [
            &self.allowed_provider_origins,
            &self.allowed_callback_origins,
        ] {
            if list.is_empty() || list.len() > 32 {
                return Err(PlatformError::Validation(
                    "identity origin list must contain 1..32 entries",
                ));
            }
            for origin in list {
                let url = Self::parse(origin)?;
                if url.origin().ascii_serialization() != *origin || url.path() != "/" {
                    return Err(PlatformError::Validation(
                        "identity policy requires canonical origins",
                    ));
                }
            }
        }
        Ok(())
    }
    fn parse(value: &str) -> Result<url::Url, PlatformError> {
        if value.len() > 2048 {
            return Err(PlatformError::Validation("identity URL exceeds limit"));
        }
        let url = url::Url::parse(value)
            .map_err(|_| PlatformError::Validation("invalid identity URL"))?;
        let local = url.host_str() == Some("localhost")
            || url
                .host_str()
                .and_then(|h| h.parse::<std::net::IpAddr>().ok())
                .is_some_and(|ip| ip.is_loopback());
        if !(url.scheme() == "https" || url.scheme() == "http" && local)
            || url.username() != ""
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(PlatformError::Validation(
                "identity URL must use HTTPS or explicitly allowed IP loopback HTTP",
            ));
        }
        Ok(url)
    }
    pub fn provider_url(&self, value: &str) -> Result<url::Url, PlatformError> {
        let url = Self::parse(value)?;
        if !self
            .allowed_provider_origins
            .contains(&url.origin().ascii_serialization())
        {
            return Err(PlatformError::Validation(
                "OIDC origin is outside operator policy",
            ));
        }
        Ok(url)
    }
    pub fn callback_url(&self, value: &str) -> Result<url::Url, PlatformError> {
        let redirect = Self::parse(value)?;
        if redirect.query().is_some()
            || redirect.path() != "/api/v1/auth/oidc/callback"
            || !self
                .allowed_callback_origins
                .contains(&redirect.origin().ascii_serialization())
        {
            return Err(PlatformError::Validation(
                "OIDC callback is outside operator policy",
            ));
        }
        Ok(redirect)
    }
    pub fn registration(&self, input: &OidcProviderRegistration) -> Result<(), PlatformError> {
        let issuer = self.provider_url(&input.issuer)?;
        if issuer.query().is_some() {
            return Err(PlatformError::Validation(
                "OIDC issuer cannot contain query",
            ));
        }
        self.callback_url(&input.redirect_uri)?;
        if input.client_id.is_empty()
            || input.client_id.len() > 512
            || input.client_secret.is_empty()
            || input.client_secret.len() > 8192
            || input.expected_revision < 0
        {
            return Err(PlatformError::Validation(
                "invalid OIDC client registration",
            ));
        }
        Ok(())
    }
}
