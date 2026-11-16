use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct ProjectContextHandlers;
impl ProjectContextHandlers {
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        body: Result<Json<ProjectContextManifestCreate>, JsonRejection>,
    ) -> Result<MutationResponse<ProjectContextManifest>, ApiError> {
        let Json(selection) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid context selection")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"selection":selection});
        let body = state
            .mutate(
                ctx,
                "context_manifest.create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_project_context_manifest(ctx, &project, &selection),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
    ) -> Result<Json<ProjectContextManifest>, ApiError> {
        state
            .run(move |store| store.get_project_context_manifest(&ctx, &project, &id))
            .await
            .map(Json)
    }
    pub async fn status(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
    ) -> Result<Json<ProjectContextStatus>, ApiError> {
        state
            .run(move |store| store.project_context_manifest_status(&ctx, &project, &id))
            .await
            .map(Json)
    }
}
