use crate::{ApiState, api_error::ApiError};
use axum::{
    Extension, Json,
    extract::{Query, State, rejection::QueryRejection},
};
use nexora_platform::*;
pub(crate) struct VisibleTaskHandlers;
impl VisibleTaskHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        query: Result<Query<VisibleTaskQuery>, QueryRejection>,
    ) -> Result<Json<VisibleTaskPage>, ApiError> {
        let Query(input) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid task query")))?;
        state
            .run(move |store| store.visible_tasks(&ctx, &input))
            .await
            .map(Json)
    }
    pub async fn types(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        query: Result<Query<crate::workflow_list_query::WorkflowListQuery>, QueryRejection>,
    ) -> Result<Json<VisibleTaskTypePage>, ApiError> {
        let Query(input) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid task type query")))?;
        state
            .run(move |store| {
                store.visible_task_types(&ctx, input.after.as_deref(), input.limit.unwrap_or(100))
            })
            .await
            .map(Json)
    }
}
