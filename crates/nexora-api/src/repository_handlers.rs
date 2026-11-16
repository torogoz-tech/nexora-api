use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
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
use nexora_platform::{
    AuthContext, PlatformError, ProjectRepository, RepositoryChange, RepositoryPage,
    RepositoryRegistration,
};
use serde_json::json;
pub(crate) struct RepositoryHandlers;
impl RepositoryHandlers {
    pub async fn register(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project_id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<RepositoryRegistration>, JsonRejection>,
    ) -> Result<MutationResponse<ProjectRepository>, ApiError> {
        let Json(registration) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid repository registration")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"project_id":project_id,"registration":registration});
        let repository = state
            .mutate(
                ctx,
                "repository.register",
                request_id.clone(),
                canonical,
                move |store, ctx| store.register_repository(ctx, &project_id, &registration),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(repository),
        })
    }
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project_id): Path<String>,
        query: Result<Query<ProjectListQuery>, QueryRejection>,
    ) -> Result<Json<RepositoryPage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid repository query")))?;
        state
            .run(move |store| {
                store.repository_page(
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
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, repository_id)): Path<(String, String)>,
    ) -> Result<Json<ProjectRepository>, ApiError> {
        state
            .run(move |store| store.get_repository(&ctx, &project_id, &repository_id))
            .await
            .map(Json)
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, repository_id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<RepositoryChange>, JsonRejection>,
    ) -> Result<MutationResponse<ProjectRepository>, ApiError> {
        let Json(change) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid repository change")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical =
            json!({"project_id":project_id,"repository_id":repository_id,"change":change});
        let repository = state
            .mutate(
                ctx,
                "repository.change",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.change_repository(ctx, &project_id, &repository_id, &change)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(repository),
        })
    }
}
