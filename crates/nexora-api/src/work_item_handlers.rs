use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
    task_history_query::TaskHistoryQuery,
    task_transition_mutation_response::TaskTransitionMutationResponse,
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
pub(crate) struct WorkItemHandlers;
impl WorkItemHandlers {
    pub async fn type_create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskTypeCreate>, JsonRejection>,
    ) -> Result<MutationResponse<TaskType>, ApiError> {
        let Json(definition) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid work-item input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"definition":definition});
        let body = state
            .mutate(
                ctx,
                "task_type.create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_task_type(ctx, &project, &definition),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn backlog_create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        body: Result<Json<BacklogCreate>, JsonRejection>,
    ) -> Result<MutationResponse<Backlog>, ApiError> {
        let Json(definition) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid work-item input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"definition":definition});
        let body = state
            .mutate(
                ctx,
                "backlog.create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_backlog(ctx, &project, &definition),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn backlog_change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<BacklogChange>, JsonRejection>,
    ) -> Result<MutationResponse<Backlog>, ApiError> {
        let Json(change) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid work-item input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"backlog_id":id,"change":change});
        let body = state
            .mutate(
                ctx,
                "backlog.change",
                request_id.clone(),
                canonical,
                move |store, ctx| store.change_backlog(ctx, &project, &id, &change),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn backlog_reorder(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<BacklogReorder>, JsonRejection>,
    ) -> Result<MutationResponse<Backlog>, ApiError> {
        let Json(order) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid work-item input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"backlog_id":id,"order":order});
        let body = state
            .mutate(
                ctx,
                "backlog.reorder",
                request_id.clone(),
                canonical,
                move |store, ctx| store.reorder_backlog(ctx, &project, &id, &order),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn task_create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskCreate>, JsonRejection>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let Json(definition) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid work-item input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"definition":definition});
        let body = state
            .mutate(
                ctx,
                "task.create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_developer_task(ctx, &project, &definition),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn task_change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskChange>, JsonRejection>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let Json(change) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid work-item input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"task_id":id,"change":change});
        let body = state
            .mutate(
                ctx,
                "task.change",
                request_id.clone(),
                canonical,
                move |store, ctx| store.change_task(ctx, &project, &id, &change),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn task_transition(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id, transition)): Path<(String, String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskTransitionRequest>, JsonRejection>,
    ) -> Result<TaskTransitionMutationResponse, ApiError> {
        let Json(parameters) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid work-item input")))?;
        let request_id = MutationId::required(&headers)?;
        let policy = state.task_workflow_git_policy();
        let response_task_id = id.clone();
        let canonical = json!({"project_id":project,"task_id":id,"transition_id":transition,"parameters":parameters});
        let (task, waiting_for_knowledge) = state
            .task_transition_mutation(
                ctx,
                "task.transition",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.transition(ctx, Some(&project), &id, &transition, &parameters, &policy)
                },
            )
            .await?;
        Ok(TaskTransitionMutationResponse {
            task,
            task_id: response_task_id,
            request_id,
            waiting_for_knowledge,
        })
    }
    pub async fn approval_grant(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskApprovalGrant>, JsonRejection>,
    ) -> Result<MutationResponse<TaskApproval>, ApiError> {
        let Json(approval) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid work-item input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"task_id":id,"approval":approval});
        let body = state
            .mutate(
                ctx,
                "task.approval_grant",
                request_id.clone(),
                canonical,
                move |store, ctx| store.grant_task_approval(ctx, &project, &id, &approval),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn task_get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
    ) -> Result<Json<WorkTask>, ApiError> {
        state
            .run(move |store| store.get_task(&ctx, &project, &id))
            .await
            .map(Json)
    }
    pub async fn backlog_get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
    ) -> Result<Json<Backlog>, ApiError> {
        state
            .run(move |store| store.get_backlog(&ctx, &project, &id))
            .await
            .map(Json)
    }
    pub async fn task_list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        query: Result<Query<TaskListQuery>, QueryRejection>,
    ) -> Result<Json<TaskPage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid task query")))?;
        state
            .run(move |store| store.task_page(&ctx, &project, &query))
            .await
            .map(Json)
    }
    pub async fn task_history(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        query: Result<Query<TaskHistoryQuery>, QueryRejection>,
    ) -> Result<Json<TaskHistoryPage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid task history query")))?;
        state
            .run(move |store| {
                store.task_history(&ctx, &project, &id, query.after, query.limit.unwrap_or(50))
            })
            .await
            .map(Json)
    }
}
