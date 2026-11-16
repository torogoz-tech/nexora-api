use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
    workflow_list_query::WorkflowListQuery, workflow_versions_query::WorkflowVersionsQuery,
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

pub(crate) struct WorkflowHandlers;
impl WorkflowHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        query: Result<Query<WorkflowListQuery>, QueryRejection>,
    ) -> Result<Json<WorkflowPage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid workflow query")))?;
        state
            .run(move |store| {
                store.workflow_page(
                    &ctx,
                    &project,
                    query.after.as_deref(),
                    query.limit.unwrap_or(50),
                    query.include_archived.unwrap_or(false),
                )
            })
            .await
            .map(Json)
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
    ) -> Result<Json<WorkflowDefinition>, ApiError> {
        state
            .run(move |store| match store.get_workflow(&ctx, &project, &id) {
                Ok(value) => Ok(value),
                Err(PlatformError::NotFound) => store.developer_workflow(&ctx, &id),
                Err(error) => Err(error),
            })
            .await
            .map(Json)
    }
    pub async fn versions(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        query: Result<Query<WorkflowVersionsQuery>, QueryRejection>,
    ) -> Result<Json<WorkflowVersionPage>, ApiError> {
        let Query(query) = query
            .map_err(|_| ApiError(PlatformError::Validation("invalid workflow version query")))?;
        state
            .run(move |store| {
                store.workflow_version_page(
                    &ctx,
                    &project,
                    &id,
                    query.before,
                    query.limit.unwrap_or(50),
                )
            })
            .await
            .map(Json)
    }
    pub async fn version(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id, version)): Path<(String, String, String)>,
    ) -> Result<Json<WorkflowVersion>, ApiError> {
        state
            .run(
                move |store| match store.workflow_version(&ctx, &project, &id, &version) {
                    Ok(value) => Ok(value),
                    Err(PlatformError::NotFound) => {
                        store.developer_workflow_version(&ctx, &id, &version)
                    }
                    Err(error) => Err(error),
                },
            )
            .await
            .map(Json)
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        body: Result<Json<WorkflowCreate>, JsonRejection>,
    ) -> Result<MutationResponse<WorkflowDefinition>, ApiError> {
        let Json(definition) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid workflow definition")))?;
        if definition
            .resource_kinds
            .contains(&WorkflowResourceKind::Task)
        {
            return Err(ApiError(PlatformError::Validation(
                "task workflows use the global workflows API",
            )));
        }
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"project_id":project,"definition":definition});
        let body = state
            .mutate(
                ctx,
                "workflow.create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_workflow(ctx, &project, &definition),
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
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<WorkflowChange>, JsonRejection>,
    ) -> Result<MutationResponse<WorkflowDefinition>, ApiError> {
        let Json(change) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid workflow change")))?;
        if change
            .resource_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.contains(&WorkflowResourceKind::Task))
        {
            return Err(ApiError(PlatformError::Validation(
                "task workflows use the global workflows API",
            )));
        }
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"project_id":project,"workflow_id":id,"change":change});
        let body = state
            .mutate(
                ctx,
                "workflow.change",
                request_id.clone(),
                canonical,
                move |store, ctx| store.change_workflow(ctx, &project, &id, &change),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn publish(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<WorkflowPublish>, JsonRejection>,
    ) -> Result<MutationResponse<WorkflowVersion>, ApiError> {
        let Json(publish) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid workflow publication")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"project_id":project,"workflow_id":id,"publish":publish});
        let body = state
            .mutate(
                ctx,
                "workflow.publish",
                request_id.clone(),
                canonical,
                move |store, ctx| store.publish_workflow(ctx, &project, &id, &publish),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
}
