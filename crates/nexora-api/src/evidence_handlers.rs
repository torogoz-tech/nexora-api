use crate::{ApiState, api_error::ApiError, mutation_id::MutationId};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_platform::{
    AuthContext, EvidenceContent, EvidenceRecord, EvidenceUpload, PlatformError,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(crate) struct EvidenceHandlers;
impl EvidenceHandlers {
    pub async fn upload(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        Json(input): Json<EvidenceUpload>,
    ) -> Result<Json<EvidenceRecord>, ApiError> {
        let id = MutationId::required(&headers)?;
        Self::save(state, ctx, project, id, input).await.map(Json)
    }
    pub async fn save(
        state: ApiState,
        ctx: AuthContext,
        project: String,
        request_id: String,
        input: EvidenceUpload,
    ) -> Result<EvidenceRecord, ApiError> {
        if input.data_base64.len() > 5592408 {
            return Err(ApiError(PlatformError::Validation("evidence exceeds4MiB")));
        }
        let bytes = STANDARD
            .decode(&input.data_base64)
            .map_err(|_| ApiError(PlatformError::Validation("invalid base64")))?;
        let mut canonical =
            serde_json::to_value(&input).map_err(|_| ApiError(PlatformError::Storage))?;
        canonical
            .as_object_mut()
            .ok_or(ApiError(PlatformError::Storage))?
            .remove("data_base64");
        canonical["sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
        canonical["project_id"] = json!(project);
        state
            .mutate(
                ctx,
                "evidence.upload",
                request_id,
                canonical,
                move |store, ctx| store.upload_evidence(ctx, &project, &input),
            )
            .await
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
    ) -> Result<Json<EvidenceRecord>, ApiError> {
        state
            .run(move |store| {
                store
                    .evidence_content(&ctx, &project, &id)
                    .map(|c| c.record)
            })
            .await
            .map(Json)
    }
    pub async fn content(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
    ) -> Result<axum::response::Response, ApiError> {
        use axum::response::IntoResponse;
        let content = state
            .run(move |store| store.evidence_content(&ctx, &project, &id))
            .await?;
        let bytes = STANDARD
            .decode(&content.data_base64)
            .map_err(|_| ApiError(PlatformError::Storage))?;
        Ok((
            [
                ("content-type", content.record.media_type),
                ("x-content-type-options", "nosniff".into()),
                ("content-disposition", "attachment".into()),
            ],
            bytes,
        )
            .into_response())
    }
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        Query(query): Query<crate::evidence_page_query::EvidencePageQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| {
                store.evidence_page(
                    &ctx,
                    &project,
                    &repository,
                    query.after.as_deref(),
                    query.limit.unwrap_or(20),
                )
            })
            .await
            .map(Json)
    }
    pub fn mcp_content(
        content: EvidenceContent,
        include: bool,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let metadata = serde_json::to_value(&content.record)
            .map_err(|_| (-32603, "Evidence encoding failed"))?;
        let mut result =
            json!({"isError":false,"content":[{"type":"text","text":metadata.to_string()}]});
        if include {
            let block = if content.record.media_type.starts_with("image/") {
                json!({"type":"image","mimeType":content.record.media_type,"data":content.data_base64})
            } else {
                let bytes = STANDARD
                    .decode(content.data_base64)
                    .map_err(|_| (-32603, "Evidence encoding failed"))?;
                json!({"type":"text","text":String::from_utf8(bytes).map_err(|_|(-32603,"Evidence encoding failed"))?})
            };
            result["content"]
                .as_array_mut()
                .ok_or((-32603, "Evidence encoding failed"))?
                .push(block);
        }
        if structured {
            result["structuredContent"] = metadata;
        }
        Ok(result)
    }
}
