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
pub(crate) struct TaskWorkLogHandlers;
impl TaskWorkLogHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(task): Path<String>,
    ) -> Result<Json<Vec<TaskWorkLog>>, ApiError> {
        state
            .run(move |store| store.task_work_logs(&ctx, &task))
            .await
            .map(Json)
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(task): Path<String>,
        headers: HeaderMap,
        Json(input): Json<TaskWorkLogInput>,
    ) -> Result<MutationResponse<TaskWorkLog>, ApiError> {
        Self::mutate(state, ctx, task, None, input, false, headers).await
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((task, id)): Path<(String, String)>,
        headers: HeaderMap,
        Json(input): Json<TaskWorkLogInput>,
    ) -> Result<MutationResponse<TaskWorkLog>, ApiError> {
        Self::mutate(state, ctx, task, Some(id), input, false, headers).await
    }
    pub async fn remove(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((task, id)): Path<(String, String)>,
        headers: HeaderMap,
        Json(input): Json<TaskWorkLogInput>,
    ) -> Result<MutationResponse<TaskWorkLog>, ApiError> {
        Self::mutate(state, ctx, task, Some(id), input, true, headers).await
    }
    async fn mutate(
        state: ApiState,
        ctx: AuthContext,
        task: String,
        id: Option<String>,
        input: TaskWorkLogInput,
        remove: bool,
        headers: HeaderMap,
    ) -> Result<MutationResponse<TaskWorkLog>, ApiError> {
        let request_id = MutationId::required(&headers)?;
        let operation = if remove {
            "task.work_log_removed"
        } else if id.is_some() {
            "task.work_log_changed"
        } else {
            "task.work_log_added"
        };
        let canonical = json!({"task_id":task,"work_log_id":id,"input":input,"remove":remove});
        let body = state
            .mutate(
                ctx,
                operation,
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.save_task_work_log(ctx, &task, id.as_deref(), &input, remove)
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
