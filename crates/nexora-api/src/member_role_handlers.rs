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
pub(crate) struct MemberRoleHandlers;
impl MemberRoleHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<MemberRoleAssignment>, ApiError> {
        state.run(move |s| s.member_role(&ctx, &id)).await.map(Json)
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<MemberRoleChange>, JsonRejection>,
    ) -> Result<MutationResponse<MemberRoleAssignment>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid member role request")))?;
        let request_id = MutationId::required(&headers)?;
        let value = state
            .mutate(
                ctx,
                "member.role.change",
                request_id.clone(),
                json!({"id":id,"input":input}),
                move |s, c| s.change_member_role(c, &id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(value),
        })
    }
}
