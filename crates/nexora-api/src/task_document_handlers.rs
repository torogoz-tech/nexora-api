use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, task_document_query::TaskDocumentQuery,
    task_document_upload::TaskDocumentUpload,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(crate) struct TaskDocumentHandlers;
impl TaskDocumentHandlers {
    pub async fn upload(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(task): Path<String>,
        headers: HeaderMap,
        Json(input): Json<TaskDocumentUpload>,
    ) -> Result<Json<Value>, ApiError> {
        if input.data_base64.len() > 27962028 {
            return Err(ApiError(PlatformError::Validation("document exceeds20MiB")));
        }
        let bytes = STANDARD
            .decode(&input.data_base64)
            .map_err(|_| ApiError(PlatformError::Validation("invalid base64")))?;
        let request = MutationId::required(&headers)?;
        let canonical = json!({"task_id":task,"name":input.name,"sha256":format!("{:x}",Sha256::digest(&bytes))});
        state
            .mutate(
                ctx,
                "task.document.upload",
                request,
                canonical,
                move |store, ctx| store.upload_task_document(ctx, &task, &input.name, &bytes),
            )
            .await
            .map(Json)
    }
    pub async fn task_page(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(task): Path<String>,
        Query(q): Query<TaskDocumentQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| {
                store.task_document_page(
                    &ctx,
                    Some(&task),
                    None,
                    q.search.as_deref().unwrap_or(""),
                    q.after.as_deref(),
                )
            })
            .await
            .map(Json)
    }
    pub async fn library(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<TaskDocumentQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| {
                store.task_document_page(
                    &ctx,
                    None,
                    q.project_id.as_deref(),
                    q.search.as_deref().unwrap_or(""),
                    q.after.as_deref(),
                )
            })
            .await
            .map(Json)
    }
    pub async fn content(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<axum::response::Response, ApiError> {
        let (_, bytes) = state
            .run(move |store| store.task_document_content(&ctx, &id))
            .await?;
        Ok((
            [
                ("content-type", "application/octet-stream"),
                ("content-disposition", "attachment"),
                ("x-content-type-options", "nosniff"),
                ("cache-control", "private, no-store"),
                ("content-security-policy", "sandbox"),
            ],
            bytes,
        )
            .into_response())
    }
}
