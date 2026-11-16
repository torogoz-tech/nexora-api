use crate::{OidcNetworkPolicy, oidc_http_client::OidcHttpClient};
use nexora_platform::{OidcProvider, PlatformError};
use openidconnect::{
    AccessTokenHash, AuthType, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    EndpointMaybeSet, EndpointNotSet, EndpointSet, IssuerUrl, Nonce, OAuth2TokenResponse,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreClientAuthMethod, CoreProviderMetadata},
};
pub(crate) struct OidcClient {
    client: CoreClient<
        EndpointSet,
        EndpointNotSet,
        EndpointNotSet,
        EndpointNotSet,
        EndpointMaybeSet,
        EndpointMaybeSet,
    >,
    http: OidcHttpClient,
    issuer: String,
}
impl OidcClient {
    pub fn connect(
        provider: &OidcProvider,
        secret: &str,
        policy: &OidcNetworkPolicy,
    ) -> Result<Self, PlatformError> {
        policy.provider_url(&provider.issuer)?;
        policy.callback_url(&provider.redirect_uri)?;
        let http = OidcHttpClient::new(policy);
        let metadata = CoreProviderMetadata::discover(
            &IssuerUrl::new(provider.issuer.clone())
                .map_err(|_| PlatformError::Validation("invalid issuer"))?,
            &|request| http.execute(request),
        )
        .map_err(|_| PlatformError::Unavailable)?;
        policy.provider_url(metadata.authorization_endpoint().as_str())?;
        policy.provider_url(metadata.jwks_uri().as_str())?;
        policy.provider_url(
            metadata
                .token_endpoint()
                .ok_or(PlatformError::Validation("OIDC token endpoint required"))?
                .as_str(),
        )?;
        let auth = if metadata
            .token_endpoint_auth_methods_supported()
            .is_some_and(|methods| methods.contains(&CoreClientAuthMethod::ClientSecretPost))
        {
            AuthType::RequestBody
        } else {
            AuthType::BasicAuth
        };
        let client = CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(provider.client_id.clone()),
            Some(ClientSecret::new(secret.to_owned())),
        )
        .set_redirect_uri(
            RedirectUrl::new(provider.redirect_uri.clone())
                .map_err(|_| PlatformError::Validation("invalid OIDC callback"))?,
        )
        .set_auth_type(auth);
        Ok(Self {
            client,
            http,
            issuer: provider.issuer.clone(),
        })
    }
    pub fn start_url(&self, state: &str, nonce: &str, verifier: &str) -> String {
        let state = state.to_owned();
        let nonce = nonce.to_owned();
        self.client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                move || CsrfToken::new(state),
                move || Nonce::new(nonce),
            )
            .add_scope(Scope::new("profile".into()))
            .add_scope(Scope::new("email".into()))
            .set_pkce_challenge(PkceCodeChallenge::from_code_verifier_sha256(
                &PkceCodeVerifier::new(verifier.into()),
            ))
            .url()
            .0
            .to_string()
    }
    pub fn exchange_subject(
        &self,
        code: &str,
        verifier: &str,
        nonce: &str,
    ) -> Result<crate::oidc_verified_identity::OidcVerifiedIdentity, PlatformError> {
        let token = self
            .client
            .exchange_code(AuthorizationCode::new(code.into()))
            .map_err(|_| PlatformError::Unauthorized)?
            .set_pkce_verifier(PkceCodeVerifier::new(verifier.into()))
            .request(&|request| self.http.execute(request))
            .map_err(|_| PlatformError::Unauthorized)?;
        let id = token.id_token().ok_or(PlatformError::Unauthorized)?;
        let validator = self.client.id_token_verifier();
        let expected_nonce = Nonce::new(nonce.into());
        let claims = id
            .claims(&validator, &expected_nonce)
            .map_err(|_| PlatformError::Unauthorized)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| PlatformError::Unavailable)?
            .as_secs() as i64;
        if claims.issuer().as_str() != self.issuer
            || claims.issue_time().timestamp() > now + 60
            || claims.subject().as_str().is_empty()
            || claims.subject().as_str().len() > 255
        {
            return Err(PlatformError::Unauthorized);
        }
        if let Some(expected) = claims.access_token_hash() {
            let actual = AccessTokenHash::from_token(
                token.access_token(),
                id.signing_alg().map_err(|_| PlatformError::Unauthorized)?,
                id.signing_key(&validator)
                    .map_err(|_| PlatformError::Unauthorized)?,
            )
            .map_err(|_| PlatformError::Unauthorized)?;
            if actual != *expected {
                return Err(PlatformError::Unauthorized);
            }
        }
        Ok(crate::oidc_verified_identity::OidcVerifiedIdentity {
            subject: claims.subject().as_str().to_owned(),
            name: claims
                .name()
                .and_then(|n| n.get(None))
                .map(|n| {
                    n.as_str()
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(160)
                        .collect()
                })
                .filter(|n: &String| !n.trim().is_empty()),
            // Unverified provider contacts are suggestions only, never recovery addresses or identity keys.
            email: claims
                .email()
                .map(|e| e.as_str())
                .or_else(|| claims.preferred_username().map(|e| e.as_str()))
                .map(str::trim)
                .filter(|e| {
                    e.len() <= 254
                        && !e.to_ascii_uppercase().contains("#EXT#")
                        && !e.chars().any(|c| c.is_control() || c.is_whitespace())
                        && email_address::EmailAddress::is_valid(e)
                })
                .map(str::to_owned),
            email_verified: claims.email().is_some() && claims.email_verified() == Some(true),
        })
    }
}
