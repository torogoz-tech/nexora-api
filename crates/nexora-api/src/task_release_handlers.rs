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
pub(crate) struct TaskReleaseHandlers;
impl TaskReleaseHandlers {
    pub async fn compositions(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.release_composition_jobs(&ctx, &project, &id))
            .await
            .map(Json)
    }
    pub async fn compose(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        Json(input): Json<TaskTake>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        if input.workflow_id.is_some() {
            return Err(ApiError(PlatformError::Validation(
                "release composition accepts no workflow",
            )));
        }
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"release_id":id,"expected_revision":input.expected_revision});
        let body = state
            .mutate(
                ctx,
                "release.compose",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.queue_release_composition(ctx, &project, &id, input.expected_revision)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::ACCEPTED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.task_releases(&ctx, &project))
            .await
            .map(Json)
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.task_release(&ctx, &project, &id))
            .await
            .map(Json)
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        Json(input): Json<Value>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        Self::save(state, ctx, project, None, headers, input).await
    }
    pub async fn add(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        Json(input): Json<Value>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        Self::save(state, ctx, project, Some(id), headers, input).await
    }
    async fn save(
        state: ApiState,
        ctx: AuthContext,
        project: String,
        id: Option<String>,
        headers: HeaderMap,
        input: Value,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"release_id":id,"input":input});
        let operation = if id.is_some() {
            "release.tasks_add"
        } else {
            "release.create"
        };
        let body = state
            .mutate(
                ctx,
                operation,
                request_id.clone(),
                canonical,
                move |store, ctx| store.save_task_release(ctx, &project, id.as_deref(), &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
