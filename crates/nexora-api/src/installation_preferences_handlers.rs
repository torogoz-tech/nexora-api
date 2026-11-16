use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct InstallationPreferencesHandlers;
impl InstallationPreferencesHandlers {
    pub async fn public_get(
        State(state): State<ApiState>,
    ) -> Result<Json<InstallationPreferences>, ApiError> {
        state
            .run(|store| store.public_installation_preferences())
            .await
            .map(Json)
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<InstallationPreferences>, ApiError> {
        state
            .run(move |store| store.installation_preferences(&ctx))
            .await
            .map(Json)
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<InstallationPreferencesChange>, JsonRejection>,
    ) -> Result<MutationResponse<InstallationPreferences>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid installation preferences",
            ))
        })?;
        let request_id = MutationId::required(&headers)?;
        let body = state
            .mutate(
                ctx,
                "installation.preferences.change",
                request_id.clone(),
                json!(input),
                move |store, ctx| store.change_installation_preferences(ctx, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
