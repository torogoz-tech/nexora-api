use crate::{ApiState, api_error::ApiError, repository_profile_query::RepositoryProfileQuery};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::QueryRejection},
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;
pub(crate) struct RepositoryProfileHandlers;
impl RepositoryProfileHandlers {
    pub async fn registered(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        query: Result<Query<RepositoryProfileQuery>, QueryRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Query(query) = query.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid repository profile query",
            ))
        })?;
        let policy = state.git_policy();
        state
            .run(move |store| {
                store.repository_profile_with_provider(
                    &ctx,
                    &project,
                    &repository,
                    query.year,
                    &policy,
                )
            })
            .await
            .map(Json)
    }
    pub async fn remote(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((connection, remote)): Path<(String, String)>,
        query: Result<Query<RepositoryProfileQuery>, QueryRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Query(query) = query.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid repository profile query",
            ))
        })?;
        let name = query.full_name.ok_or(ApiError(PlatformError::Validation(
            "repository full_name is required",
        )))?;
        let policy = state.git_policy();
        state
            .run(move |store| {
                store.remote_repository_profile(
                    &ctx,
                    &connection,
                    &remote,
                    &name,
                    query.year,
                    &policy,
                )
            })
            .await
            .map(Json)
    }
}
