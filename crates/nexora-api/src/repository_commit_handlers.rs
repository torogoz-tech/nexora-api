use crate::{ApiState, api_error::ApiError};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;
pub(crate) struct RepositoryCommitHandlers;
impl RepositoryCommitHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, sha)): Path<(String, String, String)>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let page = query
            .get("page")
            .map(|value| value.parse::<u16>())
            .transpose()
            .map_err(|_| ApiError(PlatformError::Validation("invalid commit page")))?
            .unwrap_or(1);
        let policy = state.git_policy();
        state
            .run(move |store| {
                store.repository_commit_detail(&ctx, &project, &repository, &sha, page, &policy)
            })
            .await
            .map(Json)
    }
}
