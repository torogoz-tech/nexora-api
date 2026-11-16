use crate::{ApiState, api_error::ApiError};
use axum::{
    Extension, Json,
    extract::{Query, State},
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;
pub(crate) struct DailyDeveloperAnalysisHandlers;
impl DailyDeveloperAnalysisHandlers {
    pub async fn directory(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.daily_developer_analysis_directory(&ctx))
            .await
            .map(Json)
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let target = query
            .get("member_id")
            .cloned()
            .ok_or(ApiError(PlatformError::Validation("member_id required")))?;
        let date = query
            .get("date")
            .cloned()
            .ok_or(ApiError(PlatformError::Validation("date required")))?;
        state
            .run(move |store| store.daily_developer_analysis(&ctx, &target, &date))
            .await
            .map(Json)
    }
}
