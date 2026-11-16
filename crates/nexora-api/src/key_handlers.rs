use crate::{
    ApiState, api_error::ApiError, issue_key_request::IssueKeyRequest, mutation_id::MutationId,
    mutation_response::MutationResponse, rotate_key_request::RotateKeyRequest,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{AuthContext, IssuedApiKey, PlatformError};
use serde_json::{Value, json};
pub(crate) struct KeyHandlers;
impl KeyHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        let items = state
            .run(move |store| store.list_keys(&ctx, &actor))
            .await?;
        Ok(Json(json!({"items":items})))
    }
    pub async fn issue(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
        headers: HeaderMap,
        body: Result<Json<IssueKeyRequest>, JsonRejection>,
    ) -> Result<MutationResponse<IssuedApiKey>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid key request")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"actor_id":actor,"request":input});
        let issued = state
            .mutate(
                ctx,
                "key.issue",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.issue_key(
                        ctx,
                        &actor,
                        &input.name,
                        input.scope,
                        input.expires_in_seconds,
                    )
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(issued),
        })
    }
    pub async fn rotate(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<RotateKeyRequest>, JsonRejection>,
    ) -> Result<MutationResponse<IssuedApiKey>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid rotation request")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"key_id":id,"request":input});
        let issued = state
            .mutate(
                ctx,
                "key.rotate",
                request_id.clone(),
                canonical,
                move |store, ctx| store.rotate_key(ctx, &id, input.expires_in_seconds),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(issued),
        })
    }
    pub async fn revoke(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
    ) -> Result<MutationResponse<()>, ApiError> {
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"key_id":id});
        state
            .mutate(
                ctx,
                "key.revoke",
                request_id.clone(),
                canonical,
                move |store, ctx| store.revoke_key(ctx, &id),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::NO_CONTENT,
            request_id,
            body: None,
        })
    }
}
