use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{AuthContext, WorkTask};
use serde_json::{Value, json};
pub(crate) struct TaskTransitionRunHandlers;
impl TaskTransitionRunHandlers {
    pub async fn stages(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Vec<nexora_platform::TaskStageCompletion>>, ApiError> {
        state
            .run(move |store| store.task_stages(&ctx, &id))
            .await
            .map(Json)
    }

    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.task_transition_runs(&ctx, &id))
            .await
            .map(Json)
    }
    pub async fn retry(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, run)): Path<(String, String)>,
        headers: HeaderMap,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let request_id = MutationId::required(&headers)?;
        let policy = state.git_policy();
        let result = state
            .mutate(
                ctx,
                "task.transition_retry",
                request_id.clone(),
                json!({"task_id":id,"run_id":run}),
                move |store, ctx| store.retry_task_transition(ctx, &id, &run, &policy),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(result),
        })
    }
    pub async fn branch(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, repository, branch)): Path<(String, String, String)>,
    ) -> Result<Json<Value>, ApiError> {
        let policy = state.git_policy();
        state
            .run(move |store| {
                store.task_transition_branch_fact(&ctx, &id, &repository, &branch, &policy)
            })
            .await
            .map(Json)
    }
}
