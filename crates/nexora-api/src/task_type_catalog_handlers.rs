use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
    work_catalog_page_query::WorkCatalogPageQuery,
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
pub(crate) struct TaskTypeCatalogHandlers;
impl TaskTypeCatalogHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        query: Result<Query<WorkCatalogPageQuery>, QueryRejection>,
    ) -> Result<Json<CatalogTaskTypePage>, ApiError> {
        let Query(q) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid catalog page")))?;
        state
            .run(move |store| {
                store.catalog_task_types(&ctx, q.after.as_deref(), q.limit.unwrap_or(100))
            })
            .await
            .map(Json)
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<CatalogTaskTypeCreate>, JsonRejection>,
    ) -> Result<MutationResponse<CatalogTaskType>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task type")))?;
        let request_id = MutationId::required(&headers)?;
        let body = state
            .mutate(
                ctx,
                "task_type_catalog.create",
                request_id.clone(),
                json!(input),
                move |store, ctx| store.create_catalog_task_type(ctx, &input),
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
        Path(code): Path<String>,
        headers: HeaderMap,
        body: Result<Json<CatalogTaskTypeChange>, JsonRejection>,
    ) -> Result<MutationResponse<CatalogTaskType>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task type change")))?;
        let request_id = MutationId::required(&headers)?;
        let body = state
            .mutate(
                ctx,
                "task_type_catalog.change",
                request_id.clone(),
                json!({"code":code,"change":input}),
                move |store, ctx| store.change_catalog_task_type(ctx, &code, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn selection(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
    ) -> Result<Json<ProjectTaskTypeSelection>, ApiError> {
        state
            .run(move |store| store.project_task_type_selection(&ctx, &project))
            .await
            .map(Json)
    }
    pub async fn select(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        body: Result<Json<ProjectTaskTypeSelectionChange>, JsonRejection>,
    ) -> Result<MutationResponse<ProjectTaskTypeSelection>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task type selection")))?;
        let request_id = MutationId::required(&headers)?;
        let body = state
            .mutate(
                ctx,
                "project.task_types.change",
                request_id.clone(),
                json!({"project_id":project,"selection":input}),
                move |store, ctx| store.change_project_task_type_selection(ctx, &project, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
