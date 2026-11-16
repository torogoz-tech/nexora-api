use crate::{
    ApiState, api_error::ApiError, knowledge_image_upload::KnowledgeImageUpload,
    mutation_id::MutationId, task_evidence_image::TaskEvidenceImage,
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
pub(crate) struct KnowledgeImageHandlers;
impl KnowledgeImageHandlers {
    pub async fn upload(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        Json(input): Json<KnowledgeImageUpload>,
    ) -> Result<Json<Value>, ApiError> {
        let id = MutationId::required(&headers)?;
        let fingerprint = json!({"name":input.name,"project_id":input.project_id,"sha256":format!("{:x}",Sha256::digest(input.image_base64.as_bytes()))});
        state
            .mutate(
                ctx,
                "knowledge.image.upload",
                id,
                fingerprint,
                move |store, ctx| {
                    let bytes = TaskEvidenceImage::normalize(&input.image_base64)?;
                    store.upload_knowledge_image(
                        ctx,
                        input.project_id.as_deref(),
                        &input.name,
                        &bytes,
                    )
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
            .run(move |store| store.knowledge_image_content(&ctx, &id))
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
