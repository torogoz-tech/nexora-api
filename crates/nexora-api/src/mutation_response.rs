use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
pub(crate) struct MutationResponse<T> {
    pub status: StatusCode,
    pub request_id: String,
    pub body: Option<T>,
}
impl<T: Serialize> IntoResponse for MutationResponse<T> {
    fn into_response(self) -> Response {
        let mut response = match self.body {
            Some(body) => (self.status, Json(body)).into_response(),
            None => self.status.into_response(),
        };
        if let Ok(value) = self.request_id.parse() {
            response.headers_mut().insert("idempotency-key", value);
        }
        response
    }
}
