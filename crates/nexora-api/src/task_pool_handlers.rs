use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
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
use serde_json::{Value, json};
pub(crate) struct TaskPoolHandlers;
impl TaskPoolHandlers {
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<PoolTaskCreate>, JsonRejection>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task pool input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":input.project_id,"definition":input.definition});
        let body = state
            .mutate(
                ctx,
                "task.create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_pool_task(ctx, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskChange>, JsonRejection>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task pool input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":null,"task_id":id,"change":input});
        let body = state
            .mutate(
                ctx,
                "task.change",
                request_id.clone(),
                canonical,
                move |store, ctx| store.change_pool_task(ctx, &id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn take(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskCodeTake>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task pool input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":null,"task_id":id,"take":input});
        let policy = state.git_policy();
        let body = state
            .mutate(
                ctx,
                "task.take",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.take_task_with_repositories(ctx, &id, &input, None, &policy)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn pause(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskCodeTake>, JsonRejection>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task revision")))?;
        if input.workflow_id.is_some() {
            return Err(ApiError(PlatformError::Validation(
                "pause and resume preserve the pinned workflow",
            )));
        }
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"task_id":id,"expected_revision":input.expected_revision});
        let body = state
            .mutate(
                ctx,
                "task.pause",
                request_id.clone(),
                canonical,
                move |store, ctx| store.set_task_paused(ctx, &id, input.expected_revision, true),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn resume(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskCodeTake>, JsonRejection>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task revision")))?;
        if input.workflow_id.is_some() {
            return Err(ApiError(PlatformError::Validation(
                "pause and resume preserve the pinned workflow",
            )));
        }
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"task_id":id,"expected_revision":input.expected_revision});
        let body = state
            .mutate(
                ctx,
                "task.resume",
                request_id.clone(),
                canonical,
                move |store, ctx| store.set_task_paused(ctx, &id, input.expected_revision, false),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn project_take(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskCodeTake>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task pool input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"task_id":id,"take":input});
        let policy = state.git_policy();
        let body = state
            .mutate(
                ctx,
                "task.take",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.take_task_with_repositories(ctx, &id, &input, Some(&project), &policy)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn transition(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((id, transition)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskTransitionRequest>, JsonRejection>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task pool input")))?;
        let request_id = MutationId::required(&headers)?;
        let policy = state.git_policy();
        let canonical =
            json!({"project_id":null,"task_id":id,"transition_id":transition,"parameters":input});
        let body = state
            .mutate(
                ctx,
                "task.transition",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.transition_pool_task_with_policy(ctx, &id, &transition, &input, &policy)
                },
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
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskCommentCreate>, JsonRejection>,
    ) -> Result<MutationResponse<TaskComment>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task pool input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":null,"task_id":id,"comment":input});
        let body = state
            .mutate(
                ctx,
                "task.comment_add",
                request_id.clone(),
                canonical,
                move |store, ctx| store.add_pool_task_comment(ctx, &id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.pool_task_presentation(&ctx, &id))
            .await
            .map(Json)
    }
    pub async fn options(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        query: Result<Query<crate::task_pool_options_query::TaskPoolOptionsQuery>, QueryRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Query(input) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid task options query")))?;
        state
            .run(move |store| store.task_pool_options(&ctx, input.project_id.as_deref()))
            .await
            .map(Json)
    }
    pub async fn history(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        query: Result<Query<crate::task_history_query::TaskHistoryQuery>, QueryRejection>,
    ) -> Result<Json<TaskHistoryPage>, ApiError> {
        let Query(input) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid task history query")))?;
        state
            .run(move |store| {
                store.pool_task_history(&ctx, &id, input.after, input.limit.unwrap_or(50))
            })
            .await
            .map(Json)
    }
    pub async fn comments(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        query: Result<Query<crate::task_comment_list_query::TaskCommentListQuery>, QueryRejection>,
    ) -> Result<Json<TaskCommentPage>, ApiError> {
        let Query(input) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid task comment query")))?;
        state
            .run(move |store| {
                store.pool_task_comments(
                    &ctx,
                    &id,
                    input.after.as_deref(),
                    input.limit.unwrap_or(50),
                )
            })
            .await
            .map(Json)
    }
    pub async fn approval_grant(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskApprovalGrant>, JsonRejection>,
    ) -> Result<MutationResponse<TaskApproval>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid approval grant")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":null,"task_id":id,"grant":input});
        let body = state
            .mutate(
                ctx,
                "task.approval_grant",
                request_id.clone(),
                canonical,
                move |store, ctx| store.grant_pool_task_approval(ctx, &id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn approval_get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((task, id)): Path<(String, String)>,
    ) -> Result<Json<TaskApproval>, ApiError> {
        state
            .run(move |store| store.get_pool_task_approval(&ctx, &task, &id))
            .await
            .map(Json)
    }
    pub async fn approval_revoke(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((task, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskApprovalRevoke>, JsonRejection>,
    ) -> Result<MutationResponse<TaskApproval>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid approval revocation")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical =
            json!({"project_id":null,"task_id":task,"approval_id":id,"revocation":input});
        let body = state
            .mutate(
                ctx,
                "task.approval_revoke",
                request_id.clone(),
                canonical,
                move |store, ctx| store.revoke_pool_task_approval(ctx, &task, &id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
