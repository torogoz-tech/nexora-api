use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{AuthContext, PlatformError};
use nexora_platform_store::{BranchProtectionApply, BranchProtectionPreview};
use serde_json::{Value, json};
pub(crate) struct BranchProtectionHandlers;
impl BranchProtectionHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |s| s.repository_branch_protection(&ctx, &project, &repository))
            .await
            .map(Json)
    }
    pub async fn preview(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<BranchProtectionPreview>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid branch protection request",
            ))
        })?;
        let request_id = MutationId::required(&headers)?;
        let policy = state.git_policy();
        let value = state
            .mutate(
                ctx,
                "repository.branch_protection.preview",
                request_id.clone(),
                json!({"project":project,"repository":repository,"input":input}),
                move |s, c| {
                    s.preview_repository_branch_protection(
                        c,
                        &project,
                        &repository,
                        &input,
                        &policy,
                    )
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(value),
        })
    }
    pub async fn apply(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<BranchProtectionApply>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid branch protection request",
            ))
        })?;
        let request_id = MutationId::required(&headers)?;
        let policy = state.git_policy();
        let value = state
            .mutate(
                ctx,
                "repository.branch_protection.apply",
                request_id.clone(),
                json!({"project":project,"repository":repository,"input":input}),
                move |s, c| {
                    s.apply_repository_branch_protection(c, &project, &repository, &input, &policy)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(value),
        })
    }
}
