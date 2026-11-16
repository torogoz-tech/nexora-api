use crate::{ApiState, api_error::ApiError};
use axum::{Extension, Json, extract::State};
use nexora_platform::AuthContext;
use serde_json::Value;
pub(crate) struct EffectivePermissionsHandlers;
impl EffectivePermissionsHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.effective_permissions(&ctx))
            .await
            .map(Json)
    }
}
