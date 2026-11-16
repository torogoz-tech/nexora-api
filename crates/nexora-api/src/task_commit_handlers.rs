use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct TaskCommitHandlers;
impl TaskCommitHandlers {
    pub async fn remove(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((task, link)): Path<(String, String)>,
        headers: HeaderMap,
        Json(input): Json<TaskTake>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let request_id = MutationId::required(&headers)?;
        let canonical =
            json!({"task_id":task,"commit_id":link,"expected_revision":input.expected_revision});
        let body = state
            .mutate(
                ctx,
                "task.commit_remove",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.remove_task_commit(ctx, &task, &link, input.expected_revision)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(task): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.task_commits(&ctx, &task))
            .await
            .map(Json)
    }
    pub async fn bind(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(task): Path<String>,
        headers: HeaderMap,
        Json(input): Json<Value>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"task_id":task,"input":input});
        let body = state
            .mutate(
                ctx,
                "task.commit_bind",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.bind_task_commit(
                        ctx,
                        &task,
                        &input,
                        &nexora_platform_store::GitNetworkPolicy::default(),
                    )
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
