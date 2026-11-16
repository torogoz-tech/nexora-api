use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct EmailTemplateHandlers;
impl EmailTemplateHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<EmailTemplate>, ApiError> {
        state.run(move |s| s.email_template(&ctx)).await.map(Json)
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<EmailTemplateChange>, JsonRejection>,
    ) -> Result<MutationResponse<EmailTemplate>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid email template")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "email.template.change",
                id.clone(),
                json!(input),
                move |s, c| s.change_email_template(c, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
}
