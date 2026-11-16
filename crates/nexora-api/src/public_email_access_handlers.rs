use crate::{
    ApiState, api_error::ApiError, email_action_token_input::EmailActionTokenInput,
    password_recovery_complete::PasswordRecoveryComplete,
    password_recovery_request::PasswordRecoveryRequest,
};
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
};
use nexora_platform::{MemberInvitationAccept, PlatformError};
use serde_json::Value;
pub(crate) struct PublicEmailAccessHandlers;
impl PublicEmailAccessHandlers {
    pub async fn inspect(
        State(state): State<ApiState>,
        body: Result<Json<EmailActionTokenInput>, JsonRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid email action request")))?;
        state
            .run(move |s| s.inspect_member_invitation(&input.token))
            .await
            .map(Json)
    }
    pub async fn accept(
        State(state): State<ApiState>,
        body: Result<Json<MemberInvitationAccept>, JsonRejection>,
    ) -> Result<StatusCode, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid invitation request")))?;
        state
            .run(move |s| s.accept_member_invitation(&input))
            .await?;
        Ok(StatusCode::NO_CONTENT)
    }
    pub async fn request_recovery(
        State(state): State<ApiState>,
        body: Result<Json<PasswordRecoveryRequest>, JsonRejection>,
    ) -> Result<StatusCode, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid recovery request")))?;
        state
            .run(move |s| s.request_password_recovery(&input.email))
            .await?;
        Ok(StatusCode::ACCEPTED)
    }
    pub async fn complete_recovery(
        State(state): State<ApiState>,
        body: Result<Json<PasswordRecoveryComplete>, JsonRejection>,
    ) -> Result<StatusCode, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid recovery request")))?;
        state
            .run(move |s| s.complete_password_recovery(&input.token, &input.password))
            .await?;
        Ok(StatusCode::NO_CONTENT)
    }
}
