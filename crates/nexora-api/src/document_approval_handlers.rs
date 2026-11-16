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
pub(crate) struct DocumentApprovalHandlers;
impl DocumentApprovalHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, document_id, version_id, approval_id)): Path<(
            String,
            String,
            String,
            String,
        )>,
    ) -> Result<Json<DocumentApproval>, ApiError> {
        state
            .run(move |store| {
                store.get_document_approval(
                    &ctx,
                    &project_id,
                    &document_id,
                    &version_id,
                    &approval_id,
                )
            })
            .await
            .map(Json)
    }
    pub async fn revoke(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, document_id, version_id, approval_id)): Path<(
            String,
            String,
            String,
            String,
        )>,
        headers: HeaderMap,
        body: Result<Json<DocumentApprovalRevoke>, JsonRejection>,
    ) -> Result<MutationResponse<DocumentApproval>, ApiError> {
        let Json(revocation) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid document approval revocation",
            ))
        })?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"document_id":document_id,"version_id":version_id,"approval_id":approval_id,"revocation":revocation});
        let body = state
            .mutate(
                ctx,
                "document.approval_revoke",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.revoke_document_approval(
                        ctx,
                        &project_id,
                        &document_id,
                        &version_id,
                        &approval_id,
                        &revocation,
                    )
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
