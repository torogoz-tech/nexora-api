use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
    task_template_list_query::TaskTemplateListQuery,
    task_template_versions_query::TaskTemplateVersionsQuery,
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
pub(crate) struct TaskTemplateHandlers;
impl TaskTemplateHandlers {
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskTemplateCreate>, JsonRejection>,
    ) -> Result<MutationResponse<TaskTemplate>, ApiError> {
        let Json(definition) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid template input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"definition":definition});
        let body = state
            .mutate(
                ctx,
                "task_template.create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_task_template(ctx, &project, &definition),
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
        body: Result<Json<TaskTemplateChange>, JsonRejection>,
    ) -> Result<MutationResponse<TaskTemplate>, ApiError> {
        let Json(change) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid template input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"template_id":id,"change":change});
        let body = state
            .mutate(
                ctx,
                "task_template.change",
                request_id.clone(),
                canonical,
                move |store, ctx| store.change_task_template(ctx, &project, &id, &change),
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
        body: Result<Json<TaskTemplatePublish>, JsonRejection>,
    ) -> Result<MutationResponse<TaskTemplateVersion>, ApiError> {
        let Json(publication) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid template input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"template_id":id,"publication":publication});
        let body = state
            .mutate(
                ctx,
                "task_template.publish",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.publish_task_template_version(ctx, &project, &id, &publication)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn apply(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        body: Result<Json<TaskTemplateApply>, JsonRejection>,
    ) -> Result<MutationResponse<WorkTask>, ApiError> {
        let Json(application) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid template input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"application":application});
        let body = state
            .mutate(
                ctx,
                "task.template_apply",
                request_id.clone(),
                canonical,
                move |store, ctx| store.apply_task_template(ctx, &project, &application),
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
        Path((project, id)): Path<(String, String)>,
    ) -> Result<Json<TaskTemplate>, ApiError> {
        state
            .run(move |store| store.get_task_template(&ctx, &project, &id))
            .await
            .map(Json)
    }
    pub async fn version(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id, version)): Path<(String, String, String)>,
    ) -> Result<Json<TaskTemplateVersion>, ApiError> {
        state
            .run(move |store| store.task_template_version(&ctx, &project, &id, &version))
            .await
            .map(Json)
    }
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        query: Result<Query<TaskTemplateListQuery>, QueryRejection>,
    ) -> Result<Json<TaskTemplatePage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid template query")))?;
        state
            .run(move |store| {
                store.task_template_page(
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
    pub async fn versions(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        query: Result<Query<TaskTemplateVersionsQuery>, QueryRejection>,
    ) -> Result<Json<TaskTemplateVersionPage>, ApiError> {
        let Query(query) = query
            .map_err(|_| ApiError(PlatformError::Validation("invalid template versions query")))?;
        state
            .run(move |store| {
                store.task_template_version_page(
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
}
