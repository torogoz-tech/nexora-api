use crate::OidcNetworkPolicy;
use openidconnect::{HttpRequest, HttpResponse};
pub(crate) struct OidcHttpClient {
    agent: ureq::Agent,
    policy: OidcNetworkPolicy,
}
impl OidcHttpClient {
    pub fn new(policy: &OidcNetworkPolicy) -> Self {
        Self {
            agent: ureq::Agent::config_builder()
                .proxy(None)
                .max_redirects(0)
                .http_status_as_error(false)
                .timeout_global(Some(std::time::Duration::from_secs(10)))
                .build()
                .into(),
            policy: policy.clone(),
        }
    }
    pub fn execute(&self, request: HttpRequest) -> Result<HttpResponse, std::io::Error> {
        self.policy
            .provider_url(&request.uri().to_string())
            .map_err(|_| std::io::Error::other("OIDC endpoint outside operator policy"))?;
        let response = self
            .agent
            .run(request)
            .map_err(|_| std::io::Error::other("OIDC transport unavailable"))?;
        let (parts, mut body) = response.into_parts();
        let bytes = body
            .with_config()
            .limit(1_048_576)
            .read_to_vec()
            .map_err(|_| std::io::Error::other("OIDC response unavailable or exceeds limit"))?;
        Ok(HttpResponse::from_parts(parts, bytes))
    }
}
