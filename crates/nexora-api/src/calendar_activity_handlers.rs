use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub(crate) struct CalendarActivityHandlers;
impl CalendarActivityHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.calendar_activity(&ctx, &id))
            .await
            .map(Json)
    }

    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(query): Query<BTreeMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let invalid = || ApiError(PlatformError::Validation("invalid calendar activity query"));
        if query
            .keys()
            .any(|k| !matches!(k.as_str(), "from" | "to" | "limit" | "after"))
        {
            return Err(invalid());
        }
        let from = query.get("from").ok_or_else(invalid)?.clone();
        let to = query.get("to").ok_or_else(invalid)?.clone();
        let after = query.get("after").cloned();
        let limit = query
            .get("limit")
            .map(|v| v.parse::<u32>())
            .transpose()
            .map_err(|_| invalid())?
            .unwrap_or(100);
        state
            .run(move |s| s.calendar_activities(&ctx, &from, &to, after.as_deref(), limit))
            .await
            .map(Json)
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        Self::write(state, ctx, headers, body, None, false).await
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        Self::write(state, ctx, headers, body, Some(id), false).await
    }
    pub async fn cancel(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        Self::write(state, ctx, headers, body, Some(id), true).await
    }
    pub async fn attendance(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(target): Path<String>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid attendance response")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "calendar.activity.attendance",
                id.clone(),
                json!({"target":target,"input":input}),
                move |s, c| s.respond_calendar_activity(c, &target, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
    async fn write(
        state: ApiState,
        ctx: AuthContext,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
        target: Option<String>,
        cancel: bool,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid calendar activity")))?;
        let id = MutationId::required(&headers)?;
        let op = if cancel {
            "calendar.activity.cancel"
        } else if target.is_some() {
            "calendar.activity.change"
        } else {
            "calendar.activity.create"
        };
        let result = state
            .mutate(
                ctx,
                op,
                id.clone(),
                json!({"target":target,"input":input}),
                move |s, c| s.save_calendar_activity(c, target.as_deref(), &input, cancel),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
}
