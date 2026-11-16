use crate::{ApiState, api_error::ApiError, repository_code_request::RepositoryCodeRequest};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;

pub(crate) struct RepositoryCodeHandlers;

impl RepositoryCodeHandlers {
    /// Read-only POST: authority and cache behavior are identical to MCP code reads.
    pub async fn query(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        body: Result<Json<RepositoryCodeRequest>, JsonRejection>,
    ) -> Result<Response, ApiError> {
        let Json(request) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid code query request")))?;
        let input = request.resolve(project, repository).map_err(ApiError)?;
        let value = state
            .run(move |store| {
                store.query_repository_code(
                    &ctx,
                    &input.project_id,
                    &input.repository_id,
                    input.snapshot.as_deref(),
                    &input.query,
                )
            })
            .await?;
        let bytes = Self::encode(&value).map_err(ApiError)?;
        Ok(([("Content-Type", "application/json")], bytes).into_response())
    }

    pub(crate) fn encode(value: &Value) -> Result<Vec<u8>, PlatformError> {
        let bytes = serde_json::to_vec(value).map_err(|_| PlatformError::Storage)?;
        if bytes.len() > 1_048_576 {
            return Err(PlatformError::Validation(
                "code response exceeds1MiB; reduce the query range",
            ));
        }
        Ok(bytes)
    }
}
