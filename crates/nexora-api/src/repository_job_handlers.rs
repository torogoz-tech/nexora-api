use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::{
    AuthContext, PlatformError, RepositoryJob, RepositoryJobChange, RepositoryJobKind,
    RepositoryJobRequest,
};
use serde_json::json;

pub(crate) struct RepositoryJobHandlers;
impl RepositoryJobHandlers {
    pub async fn sync(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<RepositoryJobRequest>, JsonRejection>,
    ) -> Result<MutationResponse<RepositoryJob>, ApiError> {
        Self::enqueue(
            state,
            ctx,
            project,
            repository,
            headers,
            body,
            RepositoryJobKind::Sync,
        )
        .await
    }
    pub async fn index(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<RepositoryJobRequest>, JsonRejection>,
    ) -> Result<MutationResponse<RepositoryJob>, ApiError> {
        Self::enqueue(
            state,
            ctx,
            project,
            repository,
            headers,
            body,
            RepositoryJobKind::Index,
        )
        .await
    }
    async fn enqueue(
        state: ApiState,
        ctx: AuthContext,
        project: String,
        repository: String,
        headers: HeaderMap,
        body: Result<Json<RepositoryJobRequest>, JsonRejection>,
        kind: RepositoryJobKind,
    ) -> Result<MutationResponse<RepositoryJob>, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid repository job request")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"project_id":project,"repository_id":repository,"request":input});
        let job = state
            .mutate(
                ctx,
                kind.operation(),
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.enqueue_repository_job(ctx, &project, &repository, kind, &input)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::ACCEPTED,
            request_id,
            body: Some(job),
        })
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, job)): Path<(String, String)>,
    ) -> Result<Json<RepositoryJob>, ApiError> {
        state
            .run(move |store| store.repository_job(&ctx, &project, &job))
            .await
            .map(Json)
    }
    pub async fn cancel(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, job)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<RepositoryJobChange>, JsonRejection>,
    ) -> Result<MutationResponse<RepositoryJob>, ApiError> {
        let Json(change) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid job change")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"project_id":project,"job_id":job,"change":change});
        let result = state
            .mutate(
                ctx,
                "repository.job.cancel",
                request_id.clone(),
                canonical,
                move |store, ctx| store.cancel_repository_job(ctx, &project, &job, &change),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(result),
        })
    }
}
