use crate::{ApiState, api_error::ApiError, task_history_query::TaskHistoryQuery};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use nexora_platform::AuthContext;
use serde_json::Value;
pub(crate) struct TaskTimelineHandlers;
impl TaskTimelineHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        Query(query): Query<TaskHistoryQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.task_workflow_timeline(&ctx, &id, query.after))
            .await
            .map(Json)
    }
}
