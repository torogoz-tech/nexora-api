use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
    pull_request_links_query::PullRequestLinksQuery,
};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use nexora_platform_store::PullRequestLinksStore;
use serde_json::{Value, json};
pub(crate) struct PullRequestLinkHandlers;
impl PullRequestLinkHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, number)): Path<(String, String, u64)>,
        query: Result<Query<PullRequestLinksQuery>, QueryRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Query(q) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid link page")))?;
        Ok(Json(
            state
                .run(move |s| {
                    PullRequestLinksStore::new(s).list(
                        &ctx,
                        &project,
                        &repository,
                        number,
                        q.after.as_deref(),
                        q.limit.unwrap_or(50),
                    )
                })
                .await?,
        ))
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, number)): Path<(String, String, u64)>,
        headers: HeaderMap,
        body: Result<Json<PullRequestLinkCreate>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid pull request link")))?;
        input.validate().map_err(ApiError)?;
        let request_id = MutationId::required(&headers)?;
        let policy = state.git_policy();
        let canonical =
            json!({"project_id":project,"repository_id":repository,"number":number,"link":input});
        let value = state
            .mutate(
                ctx,
                "pull_request.link.create",
                request_id.clone(),
                canonical,
                move |s, c| {
                    PullRequestLinksStore::new(s).create(
                        c,
                        &project,
                        &repository,
                        number,
                        &input,
                        &policy,
                    )
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(value),
        })
    }
    pub async fn remove(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, number, id)): Path<(String, String, u64, String)>,
        headers: HeaderMap,
        body: Result<Json<PullRequestLinkRemove>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid link removal")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"repository_id":repository,"number":number,"link_id":id,"removal":input});
        let value = state
            .mutate(
                ctx,
                "pull_request.link.remove",
                request_id.clone(),
                canonical,
                move |s, c| {
                    PullRequestLinksStore::new(s).remove(
                        c,
                        &project,
                        &repository,
                        number,
                        &id,
                        &input,
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
}
