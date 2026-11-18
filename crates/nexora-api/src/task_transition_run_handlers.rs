use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId,
    task_transition_mutation_response::TaskTransitionMutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::HeaderMap,
};
use nexora_platform::AuthContext;
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
            .run_task_workflow(move |store| store.task_transition_runs(&ctx, &id))
            .await
            .map(Json)
    }
    pub async fn retry(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, run)): Path<(String, String)>,
        headers: HeaderMap,
    ) -> Result<TaskTransitionMutationResponse, ApiError> {
        let request_id = MutationId::required(&headers)?;
        let policy = state.task_workflow_git_policy();
        let response_task_id = id.clone();
        let (task, waiting_for_knowledge) = state
            .task_transition_mutation(
                ctx,
                "task.transition_retry",
                request_id.clone(),
                json!({"task_id":id,"run_id":run}),
                move |store, ctx| store.retry_task_transition(ctx, &id, &run, &policy),
            )
            .await?;
        Ok(TaskTransitionMutationResponse {
            task,
            task_id: response_task_id,
            request_id,
            waiting_for_knowledge,
        })
    }
    pub async fn branch(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, repository, branch)): Path<(String, String, String)>,
    ) -> Result<Json<Value>, ApiError> {
        let policy = state.task_workflow_git_policy();
        state
            .run_task_workflow(move |store| {
                store.task_transition_branch_fact(&ctx, &id, &repository, &branch, &policy)
            })
            .await
            .map(Json)
    }
}
