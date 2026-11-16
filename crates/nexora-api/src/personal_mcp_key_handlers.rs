use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{
    AuthContext, IssuedApiKey, PersonalMcpKeyCreate, PersonalMcpKeyListQuery, PlatformError,
};
use serde_json::{Value, json};

pub(crate) struct PersonalMcpKeyHandlers;
impl PersonalMcpKeyHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(query): Query<PersonalMcpKeyListQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.personal_mcp_key_page(&ctx, &query))
            .await
            .map(Json)
    }
    pub async fn issue(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<PersonalMcpKeyCreate>, JsonRejection>,
    ) -> Result<MutationResponse<IssuedApiKey>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid personal MCP key request",
            ))
        })?;
        let id = MutationId::required(&headers)?;
        let fingerprint = json!({"request":input});
        let issued = state
            .mutate(
                ctx,
                "mcp.key.issue",
                id.clone(),
                fingerprint,
                move |store, ctx| store.issue_personal_mcp_key(ctx, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id: id,
            body: Some(issued),
        })
    }
    pub async fn revoke(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
    ) -> Result<MutationResponse<()>, ApiError> {
        let request = MutationId::required(&headers)?;
        let fingerprint = json!({"key_id":id});
        state
            .mutate(
                ctx,
                "mcp.key.revoke",
                request.clone(),
                fingerprint,
                move |store, ctx| store.revoke_personal_mcp_key(ctx, &id),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::NO_CONTENT,
            request_id: request,
            body: None,
        })
    }
}
