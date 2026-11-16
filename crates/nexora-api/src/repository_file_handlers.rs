use crate::{ApiState, api_error::ApiError};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;
pub(crate) struct RepositoryFileHandlers;
impl RepositoryFileHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let path = query
            .get("path")
            .cloned()
            .ok_or(ApiError(PlatformError::Validation("path required")))?;
        let commit = query
            .get("commit")
            .cloned()
            .ok_or(ApiError(PlatformError::Validation("commit required")))?;
        let policy = state.git_policy();
        state
            .run(move |s| s.repository_file(&ctx, &project, &repository, &path, &commit, &policy))
            .await
            .map(Json)
    }
}
