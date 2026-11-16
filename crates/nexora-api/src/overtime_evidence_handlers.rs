use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct OvertimeEvidenceHandlers;
impl OvertimeEvidenceHandlers {
    pub async fn planned(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |s| s.overtime_planned_evidence(&ctx))
            .await
            .map(Json)
    }
    pub async fn candidates(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |s| s.overtime_evidence_candidates(&ctx, &id))
            .await
            .map(Json)
    }
    pub async fn append(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid overtime evidence")))?;
        let request_id = MutationId::required(&headers)?;
        let value = state
            .mutate(
                ctx,
                "labor.overtime.evidence.append",
                request_id.clone(),
                json!({"id":id,"input":input}),
                move |s, c| s.append_overtime_evidence(c, &id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(value),
        })
    }
}
