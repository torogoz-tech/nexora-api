use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct TaskRelationHandlers;
impl TaskRelationHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.task_relations(&ctx, &id))
            .await
            .map(Json)
    }
    pub async fn subtask_create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskSubtaskCreate>, JsonRejection>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task relation input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"task_id":id,"subtask":input});
        let body = state
            .mutate(
                ctx,
                "task.subtask_create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_task_subtask(ctx, &id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn subtask_link(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, child)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskSubtaskChange>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task relation input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"task_id":id,"child_task_id":child,"change":input});
        let body = state
            .mutate(
                ctx,
                "task.subtask_link",
                request_id.clone(),
                canonical,
                move |store, ctx| store.change_task_subtask(ctx, &id, &child, &input, false),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn subtask_remove(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, child)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskSubtaskChange>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task relation input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"task_id":id,"child_task_id":child,"change":input});
        let body = state
            .mutate(
                ctx,
                "task.subtask_remove",
                request_id.clone(),
                canonical,
                move |store, ctx| store.change_task_subtask(ctx, &id, &child, &input, true),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn dependency_create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskDependencyCreate>, JsonRejection>,
    ) -> Result<MutationResponse<TaskDependency>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task relation input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":null,"dependency":input});
        let body = state
            .mutate(
                ctx,
                "task.dependency_create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_pool_task_dependency(ctx, &id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn dependency_remove(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, dependency)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskDependencyChange>, JsonRejection>,
    ) -> Result<MutationResponse<TaskDependency>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task relation input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical =
            json!({"project_id":null,"task_id":id,"dependency_id":dependency,"change":input});
        let body = state
            .mutate(
                ctx,
                "task.dependency_remove",
                request_id.clone(),
                canonical,
                move |store, ctx| store.remove_pool_task_dependency(ctx, &id, &dependency, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
