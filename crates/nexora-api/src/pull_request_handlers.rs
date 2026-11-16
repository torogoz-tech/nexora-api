use crate::{ApiState, api_error::ApiError, pull_request_list_query::PullRequestListQuery};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::QueryRejection},
};
use nexora_platform::{AuthContext, PlatformError, PullRequestOperation, PullRequestQuery};
use nexora_platform_store::PullRequestStore;
use serde_json::Value;
pub(crate) struct PullRequestHandlers;
impl PullRequestHandlers {
    async fn run(
        state: ApiState,
        ctx: AuthContext,
        project: String,
        repository: String,
        number: Option<u64>,
        operation: PullRequestOperation,
        query: Result<Query<PullRequestListQuery>, QueryRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid pull request query")))?;
        let query = PullRequestQuery {
            number,
            operation,
            state: query.state.unwrap_or_else(|| "open".into()),
            page: query.page.unwrap_or(1),
            limit: query.limit.unwrap_or(20),
        };
        query.validate().map_err(ApiError)?;
        let policy = state.git_policy();
        Ok(Json(
            state
                .run(move |store| {
                    PullRequestStore::new(store).query(&ctx, &project, &repository, &query, &policy)
                })
                .await?,
        ))
    }
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        query: Result<Query<PullRequestListQuery>, QueryRejection>,
    ) -> Result<Json<Value>, ApiError> {
        Self::run(
            state,
            ctx,
            project,
            repository,
            None,
            PullRequestOperation::List,
            query,
        )
        .await
    }
    pub async fn detail(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, number)): Path<(String, String, u64)>,
        query: Result<Query<PullRequestListQuery>, QueryRejection>,
    ) -> Result<Json<Value>, ApiError> {
        Self::run(
            state,
            ctx,
            project,
            repository,
            Some(number),
            PullRequestOperation::Detail,
            query,
        )
        .await
    }
    pub async fn part(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, number, part)): Path<(String, String, u64, String)>,
        query: Result<Query<PullRequestListQuery>, QueryRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let operation = match part.as_str() {
            "files" => PullRequestOperation::Files,
            "comments" => PullRequestOperation::Comments,
            "checks" => PullRequestOperation::Checks,
            _ => return Err(ApiError(PlatformError::NotFound)),
        };
        Self::run(
            state,
            ctx,
            project,
            repository,
            Some(number),
            operation,
            query,
        )
        .await
    }
}
