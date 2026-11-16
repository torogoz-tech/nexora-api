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
pub(crate) struct MemberPositionHandlers;
impl MemberPositionHandlers {
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<MemberPositionChange>, JsonRejection>,
    ) -> Result<MutationResponse<serde_json::Value>, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid member position request")))?;
        let request_id = MutationId::required(&headers)?;
        let value = state
            .mutate(
                ctx,
                "member.position.change",
                request_id.clone(),
                json!({"id":id,"input":input}),
                move |s, c| s.change_member_position(c, &id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(value),
        })
    }
}
