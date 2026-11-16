use crate::{ApiState, api_error::ApiError, mutation_id::MutationId};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::HeaderMap,
};
use nexora_platform::{AuthContext, TaskProjectChange, WorkTask};
use serde_json::json;
pub(crate) struct TaskProjectHandlers;
impl TaskProjectHandlers {
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        Json(input): Json<TaskProjectChange>,
    ) -> Result<Json<WorkTask>, ApiError> {
        let key = MutationId::required(&headers)?;
        let canonical = json!({"task_id":id,"change":input});
        state
            .mutate(
                ctx,
                "task.project.change",
                key,
                canonical,
                move |store, ctx| store.change_task_project(ctx, &id, &input),
            )
            .await
            .map(Json)
    }
}
