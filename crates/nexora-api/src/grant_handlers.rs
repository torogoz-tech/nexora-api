use crate::{
    ApiState, api_error::ApiError, grant_request::GrantRequest, mutation_id::MutationId,
    mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::json;
pub(crate) struct GrantHandlers;
impl GrantHandlers {
    pub async fn grant(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<GrantRequest>, JsonRejection>,
    ) -> Result<MutationResponse<()>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid grant request")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"actor_id":id,"grant":input});
        state
            .mutate(
                ctx,
                "grant.add",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.grant(ctx, &id, input.capability, input.project_id.as_deref())
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::NO_CONTENT,
            request_id,
            body: None,
        })
    }
    pub async fn revoke(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<GrantRequest>, JsonRejection>,
    ) -> Result<MutationResponse<()>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid grant request")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"actor_id":id,"grant":input});
        state
            .mutate(
                ctx,
                "grant.revoke",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.revoke_grant(ctx, &id, input.capability, input.project_id.as_deref())
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::NO_CONTENT,
            request_id,
            body: None,
        })
    }
}
