use crate::{
    ApiState, api_error::ApiError, remote_repository_list_query::RemoteRepositoryListQuery,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::QueryRejection},
};
use nexora_platform::{AuthContext, PlatformError, RemoteRepositoryPage};

pub(crate) struct RemoteRepositoryHandlers;
impl RemoteRepositoryHandlers {
    pub async fn available(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(connection): Path<String>,
        query: Result<Query<RemoteRepositoryListQuery>, QueryRejection>,
    ) -> Result<Json<RemoteRepositoryPage>, ApiError> {
        let Query(query) = query
            .map_err(|_| ApiError(PlatformError::Validation("invalid remote repository query")))?;
        let policy = state.git_policy();
        state
            .run(move |store| {
                store.connection_remote_repositories(
                    &ctx,
                    &connection,
                    query.page.unwrap_or(1),
                    query.limit.unwrap_or(50),
                    &policy,
                )
            })
            .await
            .map(Json)
    }

    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, connection)): Path<(String, String)>,
        query: Result<Query<RemoteRepositoryListQuery>, QueryRejection>,
    ) -> Result<Json<RemoteRepositoryPage>, ApiError> {
        let Query(query) = query
            .map_err(|_| ApiError(PlatformError::Validation("invalid remote repository query")))?;
        let policy = state.git_policy();
        Ok(Json(
            state
                .run(move |store| {
                    store.remote_repositories(
                        &ctx,
                        &project,
                        &connection,
                        query.page.unwrap_or(1),
                        query.limit.unwrap_or(50),
                        &policy,
                    )
                })
                .await?,
        ))
    }
}
