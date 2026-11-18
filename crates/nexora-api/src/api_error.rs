use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use nexora_platform::PlatformError;
use serde_json::json;
pub(crate) struct ApiError(pub PlatformError);
impl From<PlatformError> for ApiError {
    fn from(error: PlatformError) -> Self {
        Self(error)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match self.0 {
            PlatformError::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            PlatformError::Forbidden => (StatusCode::FORBIDDEN, "FORBIDDEN"),
            PlatformError::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            PlatformError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "INVALID_INPUT"),
            PlatformError::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT"),
            PlatformError::Storage => (StatusCode::SERVICE_UNAVAILABLE, "STORAGE_UNAVAILABLE"),
            PlatformError::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "CAPACITY_UNAVAILABLE"),
            PlatformError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS"),
        };
        let mut response=(status,Json(json!({"type":format!("urn:nexora:error:{}",code.to_ascii_lowercase()),"title":self.0.to_string(),"status":status.as_u16(),"code":code}))).into_response();
        response.headers_mut().insert(
            "content-type",
            axum::http::HeaderValue::from_static("application/problem+json"),
        );
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                "www-authenticate",
                axum::http::HeaderValue::from_static("Bearer"),
            );
        }
        response
    }
}
