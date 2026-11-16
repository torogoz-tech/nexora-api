use crate::{
    ApiState, api_error::ApiError, document_export_query::DocumentExportQuery,
    document_versions_query::DocumentVersionsQuery, mutation_id::MutationId,
    mutation_response::MutationResponse,
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
pub(crate) struct DocumentHandlers;
impl DocumentHandlers {
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project_id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<ProjectDocumentCreate>, JsonRejection>,
    ) -> Result<MutationResponse<ProjectDocument>, ApiError> {
        let Json(definition) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid document input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"definition":definition});
        let body = state
            .mutate(
                ctx,
                "document.create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_project_document(ctx, &project_id, &definition),
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
        Path((project_id, document_id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<ProjectDocumentChange>, JsonRejection>,
    ) -> Result<MutationResponse<ProjectDocument>, ApiError> {
        let Json(change) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid document input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"document_id":document_id,"change":change});
        let body = state
            .mutate(
                ctx,
                "document.change",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.change_project_document(ctx, &project_id, &document_id, &change)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn revise(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, document_id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<ProjectDocumentRevise>, JsonRejection>,
    ) -> Result<MutationResponse<ProjectDocumentVersion>, ApiError> {
        let Json(revision) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid document input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical =
            json!({"project_id":project_id,"document_id":document_id,"revision":revision});
        let body = state
            .mutate(
                ctx,
                "document.revise",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.revise_project_document(ctx, &project_id, &document_id, &revision)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn transition(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, document_id, version_id, transition_id)): Path<(
            String,
            String,
            String,
            String,
        )>,
        headers: HeaderMap,
        body: Result<Json<DocumentTransitionRequest>, JsonRejection>,
    ) -> Result<MutationResponse<ProjectDocumentVersion>, ApiError> {
        let Json(transition) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid document input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"document_id":document_id,"version_id":version_id,"transition_id":transition_id,"transition":transition});
        let body = state
            .mutate(
                ctx,
                "document.transition",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.transition_document_version(
                        ctx,
                        &project_id,
                        &document_id,
                        &version_id,
                        &transition_id,
                        &transition,
                    )
                },
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
        Path((project_id, document_id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<ProjectDocumentPublish>, JsonRejection>,
    ) -> Result<MutationResponse<DocumentPublication>, ApiError> {
        let Json(publication) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid document input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical =
            json!({"project_id":project_id,"document_id":document_id,"publication":publication});
        let body = state
            .mutate(
                ctx,
                "document.publish",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.publish_project_document(ctx, &project_id, &document_id, &publication)
                },
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
        Path((project_id, document_id)): Path<(String, String)>,
    ) -> Result<Json<ProjectDocument>, ApiError> {
        state
            .run(move |store| store.get_project_document(&ctx, &project_id, &document_id))
            .await
            .map(Json)
    }
    pub async fn version_get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, document_id, version_id)): Path<(String, String, String)>,
    ) -> Result<Json<ProjectDocumentVersion>, ApiError> {
        state
            .run(move |store| store.document_version(&ctx, &project_id, &document_id, &version_id))
            .await
            .map(Json)
    }
    pub async fn versions_list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, document_id)): Path<(String, String)>,
        query: Result<Query<DocumentVersionsQuery>, QueryRejection>,
    ) -> Result<Json<DocumentVersionPage>, ApiError> {
        let Query(query) = query
            .map_err(|_| ApiError(PlatformError::Validation("invalid document version query")))?;
        state
            .run(move |store| {
                store.document_version_page(
                    &ctx,
                    &project_id,
                    &document_id,
                    query.before,
                    query.limit.unwrap_or(50),
                )
            })
            .await
            .map(Json)
    }
    pub async fn export(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, document_id, version_id)): Path<(String, String, String)>,
        query: Result<Query<DocumentExportQuery>, QueryRejection>,
    ) -> Result<Json<DocumentExport>, ApiError> {
        let Query(query) = query
            .map_err(|_| ApiError(PlatformError::Validation("invalid document export query")))?;
        state
            .run(move |store| {
                store.export_project_document(
                    &ctx,
                    &project_id,
                    &document_id,
                    &version_id,
                    query.format.as_deref().unwrap_or("json"),
                )
            })
            .await
            .map(Json)
    }
}
