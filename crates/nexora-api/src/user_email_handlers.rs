use crate::{ApiState, api_error::ApiError, user_email_request::UserEmailRequest};
use axum::{Extension, Json, extract::State, http::StatusCode};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;
pub(crate) struct UserEmailHandlers;
impl UserEmailHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.own_email(&ctx))
            .await
            .map(Json)
    }
    pub async fn request(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Json(input): Json<UserEmailRequest>,
    ) -> Result<StatusCode, ApiError> {
        let email = input
            .email
            .ok_or(ApiError(PlatformError::Validation("email is required")))?;
        state
            .run(move |store| store.request_own_email(&ctx, &email))
            .await?;
        Ok(StatusCode::ACCEPTED)
    }
    pub async fn verify(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Json(input): Json<UserEmailRequest>,
    ) -> Result<StatusCode, ApiError> {
        let code = input
            .code
            .ok_or(ApiError(PlatformError::Validation("code is required")))?;
        state
            .run(move |store| store.verify_own_email(&ctx, &code))
            .await?;
        Ok(StatusCode::NO_CONTENT)
    }
}
