use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{AuthContext, NavigationOrderChange, PlatformError};
use serde_json::Value;
pub(crate) struct NavigationOrderHandlers;
impl NavigationOrderHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.navigation_order(&ctx))
            .await
            .map(Json)
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<NavigationOrderChange>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid settings navigation order",
            ))
        })?;
        let id = MutationId::required(&headers)?;
        let canonical =
            serde_json::to_value(&input).map_err(|_| ApiError(PlatformError::Storage))?;
        let value = state
            .mutate(
                ctx,
                "settings.navigation.change",
                id.clone(),
                canonical,
                move |store, ctx| store.change_navigation_order(ctx, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(value),
        })
    }
}
