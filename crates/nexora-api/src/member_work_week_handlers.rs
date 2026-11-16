use crate::{ApiState, api_error::ApiError, member_work_week_query::MemberWorkWeekQuery};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::QueryRejection},
};
use nexora_platform::{AuthContext, PlatformError};
pub(crate) struct MemberWorkWeekHandlers;
impl MemberWorkWeekHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        query: Result<Query<MemberWorkWeekQuery>, QueryRejection>,
    ) -> Result<Json<serde_json::Value>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid work week query")))?;
        state
            .run(move |store| store.member_work_week(&ctx, &id, query.date.as_deref()))
            .await
            .map(Json)
    }
}
