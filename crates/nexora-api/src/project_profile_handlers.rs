use crate::{ApiState, api_error::ApiError, project_profile_query::ProjectProfileQuery};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use nexora_platform::AuthContext;
use serde_json::Value;
pub(crate) struct ProjectProfileHandlers;
impl ProjectProfileHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        Query(query): Query<ProjectProfileQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.project_profile(&ctx, &id, query.year))
            .await
            .map(Json)
    }
}
