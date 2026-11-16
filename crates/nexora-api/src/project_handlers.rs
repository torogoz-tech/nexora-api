use crate::{
    ApiState, api_error::ApiError, create_project_request::CreateProjectRequest,
    mutation_id::MutationId, mutation_response::MutationResponse,
    project_list_query::ProjectListQuery,
};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{AuthContext, PlatformError, Project, ProjectChange, ProjectPage};
use serde_json::json;
pub(crate) struct ProjectHandlers;
impl ProjectHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        query: Result<Query<ProjectListQuery>, QueryRejection>,
    ) -> Result<Json<ProjectPage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid project query")))?;
        state
            .run(move |store| {
                store.project_page(
                    &ctx,
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
        Path(id): Path<String>,
    ) -> Result<Json<Project>, ApiError> {
        state
            .run(move |store| store.get_project(&ctx, &id))
            .await
            .map(Json)
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<ProjectChange>, JsonRejection>,
    ) -> Result<MutationResponse<Project>, ApiError> {
        let Json(change) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid project change")))?;
        let request_id = MutationId::read(&headers)?;
        let input = json!({"id":id,"change":change});
        let project = state
            .mutate(
                ctx,
                "project.change",
                request_id.clone(),
                input,
                move |store, ctx| store.change_project(ctx, &id, &change),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(project),
        })
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<CreateProjectRequest>, JsonRejection>,
    ) -> Result<MutationResponse<Project>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid project request")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = serde_json::to_value(&input)
            .map_err(|_| ApiError(PlatformError::Validation("invalid project request")))?;
        let project = state
            .mutate(
                ctx,
                "project.create",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.create_project_with_color(
                        ctx,
                        &input.name,
                        input.description.as_deref(),
                        input.color.as_ref(),
                    )
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(project),
        })
    }
}
