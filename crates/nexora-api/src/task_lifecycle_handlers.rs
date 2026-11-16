use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct TaskLifecycleHandlers;
impl TaskLifecycleHandlers {
    pub async fn movement(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, task_id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskMove>, JsonRejection>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let Json(movement) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task movement")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"task_id":task_id,"movement":movement});
        let body = state
            .mutate(
                ctx,
                "task.move",
                request_id.clone(),
                canonical,
                move |store, ctx| store.move_task(ctx, &project_id, &task_id, &movement),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn migration(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, task_id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskWorkflowMigration>, JsonRejection>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let Json(migration) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid task workflow migration")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"task_id":task_id,"migration":migration});
        let body = state
            .mutate(
                ctx,
                "task.workflow_migrate",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.migrate_task_workflow(ctx, &project_id, &task_id, &migration)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
