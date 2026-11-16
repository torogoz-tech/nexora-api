use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId,
    rating_evidence_upload::RatingEvidenceUpload, task_evidence_image::TaskEvidenceImage,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::HeaderMap,
    response::IntoResponse,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(crate) struct RatingEvidenceHandlers;
impl RatingEvidenceHandlers {
    pub async fn upload(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
        headers: HeaderMap,
        Json(input): Json<RatingEvidenceUpload>,
    ) -> Result<Json<Value>, ApiError> {
        if input.data_base64.len() > 27962028 {
            return Err(ApiError(PlatformError::Validation(
                "evidence exceeds 20 MiB",
            )));
        }
        let request = MutationId::required(&headers)?;
        let canonical = json!({"target":actor,"project":input.project_id,"name":input.name,"image":input.image,"sha256":format!("{:x}",Sha256::digest(input.data_base64.as_bytes()))});
        state
            .mutate(
                ctx,
                "rating.evidence.upload",
                request,
                canonical,
                move |store, ctx| {
                    let bytes = if input.image {
                        TaskEvidenceImage::normalize(&input.data_base64)?
                    } else {
                        STANDARD
                            .decode(&input.data_base64)
                            .map_err(|_| PlatformError::Validation("invalid base64"))?
                    };
                    store.upload_rating_evidence(
                        ctx,
                        &actor,
                        &input.project_id,
                        &input.name,
                        input.image,
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
    ) -> Result<axum::response::Response, ApiError> {
        let (image, bytes) = state
            .run(move |store| store.rating_evidence_content(&ctx, &id))
            .await?;
        Ok((
            [
                (
                    "content-type",
                    if image {
                        "image/png"
                    } else {
                        "application/octet-stream"
                    },
                ),
                (
                    "content-disposition",
                    if image {
                        "inline; filename=\"evidence.png\""
                    } else {
                        "attachment"
                    },
                ),
                ("x-content-type-options", "nosniff"),
                ("cache-control", "private, no-store"),
                ("content-security-policy", "sandbox"),
            ],
            bytes,
        )
            .into_response())
    }
}
