use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
    smtp_mailer::SmtpMailer, smtp_test_input::SmtpTestInput,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct SmtpTestHandlers;
impl SmtpTestHandlers {
    pub async fn queue(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<SmtpTestInput>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid test email")))?;
        if input.recipient.len() > 320 {
            return Err(ApiError(PlatformError::Validation("invalid test email")));
        }
        SmtpMailer::mailbox(&input.recipient)?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "smtp.test",
                id.clone(),
                json!({"recipient":input.recipient}),
                move |s, c| {
                    s.installation_service_settings(c, InstallationService::Smtp)?;
                    let config = s
                        .smtp_runtime_settings()?
                        .ok_or(PlatformError::Unavailable)?;
                    let metadata: SmtpConfiguration = serde_json::from_value(config.settings)
                        .map_err(|_| PlatformError::Storage)?;
                    SmtpMailer::mailbox(&metadata.sender_email)?;
                    if metadata.host.is_empty()
                        || !metadata.username.is_empty() && config.secret.is_none()
                    {
                        return Err(PlatformError::Validation("SMTP credentials missing"));
                    }
                    s.queue_smtp_test(c, &input.recipient)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::ACCEPTED,
            request_id: id,
            body: Some(result),
        })
    }
    pub async fn status(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |s| s.smtp_test_status(&ctx, &id))
            .await
            .map(Json)
    }
}
