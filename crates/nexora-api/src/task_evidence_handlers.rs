use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, task_evidence_image::TaskEvidenceImage,
    task_evidence_upload::TaskEvidenceUpload,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::HeaderMap,
    response::IntoResponse,
};
use nexora_platform::AuthContext;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(crate) struct TaskEvidenceHandlers;
impl TaskEvidenceHandlers {
    pub async fn upload(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        Json(input): Json<TaskEvidenceUpload>,
    ) -> Result<Json<Value>, ApiError> {
        let id = MutationId::required(&headers)?;
        let fingerprint = json!({"name":input.name,"sha256":format!("{:x}",Sha256::digest(input.image_base64.as_bytes()))});
        state
            .mutate(
                ctx,
                "task.evidence.upload",
                id,
                fingerprint,
                move |store, ctx| {
                    let bytes = TaskEvidenceImage::normalize(&input.image_base64)?;
                    store.upload_task_evidence(ctx, &input.name, &bytes)
                },
            )
            .await
            .map(Json)
    }
    pub async fn content(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<impl IntoResponse, ApiError> {
        let bytes = state
            .run(move |store| store.task_evidence_content(&ctx, &id))
            .await?;
        Ok((
            [
                ("content-type", "image/png"),
                ("x-content-type-options", "nosniff"),
                ("cache-control", "private, no-store"),
                ("content-disposition", "inline; filename=\"evidence.png\""),
            ],
            bytes,
        ))
    }
}
