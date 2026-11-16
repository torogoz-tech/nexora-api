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
use serde_json::json;
pub(crate) struct DeveloperWorkflowMemberHandlers;
impl DeveloperWorkflowMemberHandlers {
    pub async fn directory(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        query: Result<Query<crate::workflow_list_query::WorkflowListQuery>, QueryRejection>,
    ) -> Result<Json<DeveloperDirectoryPage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid developer page")))?;
        state
            .run(move |store| {
                store.developer_directory(&ctx, query.after.as_deref(), query.limit.unwrap_or(100))
            })
            .await
            .map(Json)
    }
    pub async fn board(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        query: Result<Query<crate::workflow_list_query::WorkflowListQuery>, QueryRejection>,
    ) -> Result<Json<DeveloperBoardPage>, ApiError> {
        let Query(query) = query
            .map_err(|_| ApiError(PlatformError::Validation("invalid developer task page")))?;
        state
            .run(move |store| {
                store.developer_board_tasks(
                    &ctx,
                    query.after.as_deref(),
                    query.limit.unwrap_or(100),
                )
            })
            .await
            .map(Json)
    }

    pub async fn members(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Vec<DeveloperWorkflowAssignment>>, ApiError> {
        state
            .run(move |store| store.developer_workflow_members(&ctx, &id))
            .await
            .map(Json)
    }
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
    ) -> Result<Json<Vec<DeveloperWorkflowAssignment>>, ApiError> {
        state
            .run(move |store| store.actor_workflows(&ctx, &actor))
            .await
            .map(Json)
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
    ) -> Result<Json<Option<DeveloperWorkflowAssignment>>, ApiError> {
        state
            .run(move |store| store.actor_workflow(&ctx, &actor))
            .await
            .map(Json)
    }
    pub async fn assign(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
        headers: HeaderMap,
        body: Result<Json<DeveloperWorkflowChange>, JsonRejection>,
    ) -> Result<MutationResponse<DeveloperWorkflowAssignment>, ApiError> {
        let Json(change) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid developer workflow assignment",
            ))
        })?;
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"actor_id":actor,"assignment":change});
        let body = state
            .mutate(
                ctx,
                "developer.workflow_assign",
                request_id.clone(),
                canonical,
                move |store, ctx| store.assign_actor_workflow(ctx, &actor, &change),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn remove(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
        headers: HeaderMap,
        body: Result<Json<DeveloperWorkflowRemoval>, JsonRejection>,
    ) -> Result<MutationResponse<()>, ApiError> {
        let Json(removal) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid membership removal")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"actor_id":actor,"removal":removal});
        state
            .mutate(
                ctx,
                "developer.workflow_remove",
                request_id.clone(),
                canonical,
                move |store, ctx| store.remove_actor_workflow(ctx, &actor, &removal),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::NO_CONTENT,
            request_id,
            body: None,
        })
    }
}
