use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
    task_comment_list_query::TaskCommentListQuery,
};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct TaskCollaborationHandlers;
impl TaskCollaborationHandlers {
    pub async fn dependency_create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskDependencyCreate>, JsonRejection>,
    ) -> Result<MutationResponse<TaskDependency>, ApiError> {
        let Json(dependency) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid task collaboration input",
            ))
        })?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"dependency":dependency});
        let body = state
            .mutate(
                ctx,
                "task.dependency_create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_task_dependency(ctx, &project, &dependency),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn dependency_resolve(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskDependencyChange>, JsonRejection>,
    ) -> Result<MutationResponse<TaskDependency>, ApiError> {
        let Json(change) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid task collaboration input",
            ))
        })?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"dependency_id":id,"change":change});
        let body = state
            .mutate(
                ctx,
                "task.dependency_resolve",
                request_id.clone(),
                canonical,
                move |store, ctx| store.resolve_task_dependency(ctx, &project, &id, &change),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn dependency_remove(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskDependencyChange>, JsonRejection>,
    ) -> Result<MutationResponse<TaskDependency>, ApiError> {
        let Json(change) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid task collaboration input",
            ))
        })?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"dependency_id":id,"change":change});
        let body = state
            .mutate(
                ctx,
                "task.dependency_remove",
                request_id.clone(),
                canonical,
                move |store, ctx| store.remove_task_dependency(ctx, &project, &id, &change),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn comment_add(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, task)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskCommentCreate>, JsonRejection>,
    ) -> Result<MutationResponse<TaskComment>, ApiError> {
        let Json(comment) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid task collaboration input",
            ))
        })?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"task_id":task,"comment":comment});
        let body = state
            .mutate(
                ctx,
                "task.comment_add",
                request_id.clone(),
                canonical,
                move |store, ctx| store.add_task_comment(ctx, &project, &task, &comment),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn dependencies(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, task)): Path<(String, String)>,
        query: Result<Query<TaskDependencyQuery>, QueryRejection>,
    ) -> Result<Json<TaskDependencyPage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid dependency query")))?;
        state
            .run(move |store| store.task_dependency_page(&ctx, &project, &task, &query))
            .await
            .map(Json)
    }
    pub async fn comments(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, task)): Path<(String, String)>,
        query: Result<Query<TaskCommentListQuery>, QueryRejection>,
    ) -> Result<Json<TaskCommentPage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid comment query")))?;
        state
            .run(move |store| {
                store.task_comment_page(
                    &ctx,
                    &project,
                    &task,
                    query.after.as_deref(),
                    query.limit.unwrap_or(50),
                )
            })
            .await
            .map(Json)
    }
}
