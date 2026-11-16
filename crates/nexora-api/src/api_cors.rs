use axum::http::{HeaderName, HeaderValue, Method, header};
use nexora_platform::PlatformError;
use tower_http::cors::{AllowOrigin, CorsLayer};
/// Operator-selected exact origins. Does not replace bearer authentication.
pub struct ApiCors;
impl ApiCors {
    pub fn layer(origins: &[String]) -> Result<CorsLayer, PlatformError> {
        if origins.len() > 16 {
            return Err(PlatformError::Validation("too many CORS origins"));
        }
        let mut headers = Vec::new();
        for origin in origins {
            // CORS permits explicitly configured localhost HTTP without relaxing
            // the OIDC network policy. Validate its syntax as IP loopback,
            // while preserving the original hostname for exact CORS matching.
            let validation_origin = origin
                .strip_prefix("http://localhost")
                .filter(|suffix| suffix.is_empty() || suffix.starts_with(':'))
                .map(|suffix| format!("http://127.0.0.1{suffix}"))
                .unwrap_or_else(|| origin.clone());
            let policy = nexora_platform_store::OidcNetworkPolicy {
                allowed_callback_origins: vec![validation_origin],
                ..Default::default()
            };
            policy.validate()?;
            headers.push(
                HeaderValue::from_str(origin)
                    .map_err(|_| PlatformError::Validation("invalid CORS origin"))?,
            );
        }
        Ok(CorsLayer::new()
            .allow_origin(AllowOrigin::list(headers))
            .allow_methods([
                Method::GET,
                Method::HEAD,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                header::AUTHORIZATION,
                HeaderName::from_static("x-nexora-browser"),
                header::CONTENT_TYPE,
                HeaderName::from_static("idempotency-key"),
                HeaderName::from_static("mcp-protocol-version"),
                HeaderName::from_static("mcp-session-id"),
                HeaderName::from_static("last-event-id"),
            ])
            .expose_headers([
                HeaderName::from_static("x-request-id"),
                HeaderName::from_static("mcp-session-id"),
            ])
            .max_age(std::time::Duration::from_secs(600)))
    }
}
