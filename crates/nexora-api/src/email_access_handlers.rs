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
pub(crate) struct EmailAccessHandlers;
impl EmailAccessHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<EmailAccessSettings>, ApiError> {
        state
            .run(move |s| s.email_access_settings(&ctx))
            .await
            .map(Json)
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<EmailAccessChange>, JsonRejection>,
    ) -> Result<MutationResponse<EmailAccessSettings>, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid public web URL request")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "email.access.change",
                id.clone(),
                json!(input),
                move |s, c| s.change_email_access_settings(c, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
}
