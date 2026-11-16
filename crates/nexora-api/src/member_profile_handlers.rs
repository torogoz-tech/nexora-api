use crate::{ApiState, api_error::ApiError, member_profile_query::MemberProfileQuery};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use nexora_platform::AuthContext;
use serde_json::Value;
pub(crate) struct MemberProfileHandlers;
impl MemberProfileHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        Query(query): Query<MemberProfileQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.member_profile(&ctx, &id, query.year))
            .await
            .map(Json)
    }
}
