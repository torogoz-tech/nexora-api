use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct OvertimeHandlers;
impl OvertimeHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state.run(move |s| s.overtime(&ctx)).await.map(Json)
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid overtime request")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "overtime.request.create",
                id.clone(),
                json!(input),
                move |s, c| s.create_overtime_request(c, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
    pub async fn decide(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(target): Path<String>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid overtime decision")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "overtime.request.decide",
                id.clone(),
                json!({"target":target,"input":input}),
                move |s, c| s.decide_overtime_request(c, &target, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
}
