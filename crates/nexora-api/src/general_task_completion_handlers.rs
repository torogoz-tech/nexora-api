use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
pub(crate) struct GeneralTaskCompletionHandlers;
impl GeneralTaskCompletionHandlers {
    pub async fn complete(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(task): Path<String>,
        headers: HeaderMap,
        Json(input): Json<TaskTake>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        if input.workflow_id.is_some() {
            return Err(ApiError(PlatformError::Validation(
                "general completion accepts no workflow",
            )));
        }
        let request_id = MutationId::required(&headers)?;
        let canonical =
            serde_json::json!({"task_id":task,"expected_revision":input.expected_revision});
        let body = state
            .mutate(
                ctx,
                "task.general_complete",
                request_id.clone(),
                canonical,
                move |store, ctx| store.complete_general_task(ctx, &task, input.expected_revision),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
