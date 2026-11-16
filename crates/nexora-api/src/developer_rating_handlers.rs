use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct DeveloperRatingHandlers;
impl DeveloperRatingHandlers {
    pub async fn own_weekly(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |s| s.own_weekly_rating(&ctx))
            .await
            .map(Json)
    }

    pub async fn directory(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let after = query.get("after").cloned();
        let limit = query
            .get("limit")
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(100);
        state
            .run(move |s| s.rating_directory(&ctx, after.as_deref(), limit))
            .await
            .map(Json)
    }

    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let year = query
            .get("year")
            .and_then(|v| v.parse::<i32>().ok())
            .ok_or(ApiError(PlatformError::Validation("year is required")))?;
        state
            .run(move |s| s.member_ratings(&ctx, &actor, year))
            .await
            .map(Json)
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
        headers: HeaderMap,
        body: Result<Json<Value>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid rating request")))?;
        let request_id = MutationId::required(&headers)?;
        let value = state
            .mutate(
                ctx,
                "member.rating.create",
                request_id.clone(),
                json!({"actor_id":actor,"input":input}),
                move |s, c| s.create_member_rating(c, &actor, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(value),
        })
    }
}
