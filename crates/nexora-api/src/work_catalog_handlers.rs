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
pub(crate) struct WorkCatalogHandlers;
impl WorkCatalogHandlers {
    pub async fn task_types_list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project_id): Path<String>,
        query: Result<Query<WorkCatalogPageQuery>, QueryRejection>,
    ) -> Result<Json<TaskTypePage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid work catalog page")))?;
        state
            .run(move |store| {
                store.task_type_page(
                    &ctx,
                    &project_id,
                    query.after.as_deref(),
                    query.limit.unwrap_or(50),
                    query.include_archived.unwrap_or(false),
                )
            })
            .await
            .map(Json)
    }
    pub async fn backlogs_list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project_id): Path<String>,
        query: Result<Query<WorkCatalogPageQuery>, QueryRejection>,
    ) -> Result<Json<BacklogPage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid work catalog page")))?;
        state
            .run(move |store| {
                store.backlog_page(
                    &ctx,
                    &project_id,
                    query.after.as_deref(),
                    query.limit.unwrap_or(50),
                    query.include_archived.unwrap_or(false),
                )
            })
            .await
            .map(Json)
    }
    pub async fn task_type_change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, type_code)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskTypeChange>, JsonRejection>,
    ) -> Result<MutationResponse<TaskType>, ApiError> {
        let Json(change) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid task type change")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"type_code":type_code,"change":change});
        let body = state
            .mutate(
                ctx,
                "task_type.change",
                request_id.clone(),
                canonical,
                move |store, ctx| store.change_task_type(ctx, &project_id, &type_code, &change),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
