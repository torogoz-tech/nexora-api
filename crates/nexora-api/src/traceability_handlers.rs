use crate::{
    ApiState, api_error::ApiError, document_export_query::DocumentExportQuery,
    document_link_page_query::DocumentLinkPageQuery, mutation_id::MutationId,
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
pub(crate) struct TraceabilityHandlers;
impl TraceabilityHandlers {
    pub async fn document_link_create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, document_id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<DocumentLinkCreate>, JsonRejection>,
    ) -> Result<MutationResponse<DocumentLink>, ApiError> {
        let Json(link) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid traceability input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"document_id":document_id,"link":link});
        let body = state
            .mutate(
                ctx,
                "document.link_create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_document_link(ctx, &project_id, &document_id, &link),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn document_link_remove(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, document_id, link_id)): Path<(String, String, String)>,
        headers: HeaderMap,
        body: Result<Json<DocumentLinkRemove>, JsonRejection>,
    ) -> Result<MutationResponse<DocumentLink>, ApiError> {
        let Json(removal) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid traceability input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"document_id":document_id,"link_id":link_id,"removal":removal});
        let body = state
            .mutate(
                ctx,
                "document.link_remove",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.remove_document_link(ctx, &project_id, &document_id, &link_id, &removal)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn context_package_create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project_id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<ProjectContextPackageCreate>, JsonRejection>,
    ) -> Result<MutationResponse<ProjectContextPackage>, ApiError> {
        let Json(selection) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid traceability input")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"selection":selection});
        let body = state
            .mutate(
                ctx,
                "context_package.create",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.create_project_context_package(ctx, &project_id, &selection)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn document_links_list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, document_id)): Path<(String, String)>,
        query: Result<Query<DocumentLinkPageQuery>, QueryRejection>,
    ) -> Result<Json<DocumentLinkPage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid link page query")))?;
        state
            .run(move |store| {
                store.document_links_page(
                    &ctx,
                    &project_id,
                    &document_id,
                    query.after.as_deref(),
                    query.limit.unwrap_or(50),
                    query.include_removed.unwrap_or(false),
                )
            })
            .await
            .map(Json)
    }
    pub async fn task_documents_list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, task_id)): Path<(String, String)>,
        query: Result<Query<DocumentLinkPageQuery>, QueryRejection>,
    ) -> Result<Json<DocumentLinkPage>, ApiError> {
        let Query(query) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid link page query")))?;
        state
            .run(move |store| {
                store.task_document_links_page(
                    &ctx,
                    &project_id,
                    &task_id,
                    query.after.as_deref(),
                    query.limit.unwrap_or(50),
                    query.include_removed.unwrap_or(false),
                )
            })
            .await
            .map(Json)
    }
    pub async fn context_package_get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, package_id)): Path<(String, String)>,
    ) -> Result<Json<ProjectContextPackage>, ApiError> {
        state
            .run(move |store| store.get_project_context_package(&ctx, &project_id, &package_id))
            .await
            .map(Json)
    }
    pub async fn context_package_export(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, package_id)): Path<(String, String)>,
        query: Result<Query<DocumentExportQuery>, QueryRejection>,
    ) -> Result<Json<ContextPackageExport>, ApiError> {
        let Query(query) = query
            .map_err(|_| ApiError(PlatformError::Validation("invalid package export query")))?;
        state
            .run(move |store| {
                store.export_project_context_package(
                    &ctx,
                    &project_id,
                    &package_id,
                    query.format.as_deref().unwrap_or("json"),
                )
            })
            .await
            .map(Json)
    }
}
