use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct TaskWorkflowMigrationHandlers;
impl TaskWorkflowMigrationHandlers {
    pub async fn migrate(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        h: HeaderMap,
        Json(v): Json<TaskWorkflowMigration>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let request_id = MutationId::required(&h)?;
        let canonical = json!({"task_id":id,"migration":v});
        let body = s
            .mutate(
                ctx,
                "task.workflow_migrate",
                request_id.clone(),
                canonical,
                move |s, ctx| s.migrate_pool_task_workflow(ctx, &id, &v),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
