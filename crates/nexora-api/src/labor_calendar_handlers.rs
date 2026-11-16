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
pub(crate) struct LaborCalendarHandlers;
impl LaborCalendarHandlers {
    pub async fn settings(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |s| s.labor_calendar_settings(&ctx))
            .await
            .map(Json)
    }
    pub async fn calendar(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state.run(move |s| s.labor_calendar(&ctx)).await.map(Json)
    }
    pub async fn change_settings(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid labor calendar request")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "labor.calendar.change",
                id.clone(),
                json!(input),
                move |s, c| s.change_labor_calendar(c, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
    pub async fn create_request(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid labor calendar request")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "labor.request.create",
                id.clone(),
                json!(input),
                move |s, c| s.create_labor_leave_request(c, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
    pub async fn propose_holiday(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid labor calendar request")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "labor.holiday.propose",
                id.clone(),
                json!(input),
                move |s, c| s.propose_labor_holiday(c, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
    pub async fn assign_approver(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(target): Path<String>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid labor calendar request")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "labor.approver.assign",
                id.clone(),
                json!({"target":target,"input":input}),
                move |s, c| s.assign_labor_approver(c, &target, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
    pub async fn decide_request(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(target): Path<String>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid labor calendar request")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "labor.request.decide",
                id.clone(),
                json!({"target":target,"input":input}),
                move |s, c| s.decide_labor_request(c, &target, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
    pub async fn decide_holiday(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(target): Path<String>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid labor calendar request")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "labor.holiday.decide",
                id.clone(),
                json!({"target":target,"input":input}),
                move |s, c| s.decide_labor_holiday(c, &target, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
}
