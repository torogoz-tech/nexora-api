use crate::{ApiState, api_error::ApiError};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use nexora_platform::{AuthContext, PlatformError};
use nexora_platform_store::{InAppNotificationChange, InAppNotificationQuery};
use serde_json::Value;

pub(crate) struct InAppNotificationHandlers;
impl InAppNotificationHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(query): Query<InAppNotificationQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.in_app_notifications(&ctx, &query))
            .await
            .map(Json)
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        body: Result<Json<InAppNotificationChange>, JsonRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Json(change) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid notification change")))?;
        state
            .run(move |store| store.change_in_app_notification(&ctx, &id, &change))
            .await
            .map(Json)
    }
    pub async fn read_all(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.read_all_in_app_notifications(&ctx))
            .await
            .map(Json)
    }
}
