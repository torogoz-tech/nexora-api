use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct EmailContentHandlers;
impl EmailContentHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Vec<EmailContentTemplate>>, ApiError> {
        state.run(move |s| s.email_contents(&ctx)).await.map(Json)
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((kind, language)): Path<(String, String)>,
    ) -> Result<Json<EmailContentTemplate>, ApiError> {
        state
            .run(move |s| s.email_content(&ctx, &kind, &language))
            .await
            .map(Json)
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((kind, language)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<EmailContentChange>, JsonRejection>,
    ) -> Result<MutationResponse<EmailContentTemplate>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid email content")))?;
        let id = MutationId::required(&headers)?;
        let fingerprint = json!({"kind":kind,"language":language,"input":input});
        let result = state
            .mutate(
                ctx,
                "email.content.change",
                id.clone(),
                fingerprint,
                move |s, c| s.change_email_content(c, &kind, &language, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
}
