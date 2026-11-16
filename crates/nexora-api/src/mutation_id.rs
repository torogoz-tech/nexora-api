use crate::api_error::ApiError;
use axum::http::HeaderMap;
use nexora_platform::PlatformError;
use nexora_platform_store::SecretGenerator;
pub(crate) struct MutationId;
impl MutationId {
    pub fn required(headers: &HeaderMap) -> Result<String, ApiError> {
        if !headers.contains_key("idempotency-key") {
            return Err(ApiError(PlatformError::Validation(
                "idempotency key required",
            )));
        }
        Self::read(headers)
    }
    pub fn read(headers: &HeaderMap) -> Result<String, ApiError> {
        match headers.get("idempotency-key") {
            Some(value) => value
                .to_str()
                .map(str::to_owned)
                .map_err(|_| ApiError(PlatformError::Validation("invalid idempotency key"))),
            None => SecretGenerator::id("idp").map_err(ApiError),
        }
    }
}
