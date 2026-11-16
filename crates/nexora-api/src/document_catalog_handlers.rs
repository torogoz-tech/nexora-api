use crate::{
    ApiState, api_error::ApiError, document_history_query::DocumentHistoryQuery,
    document_publications_query::DocumentPublicationsQuery,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::QueryRejection},
};
use nexora_platform::*;
pub(crate) struct DocumentCatalogHandlers;
impl DocumentCatalogHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        query: Result<Query<DocumentListQuery>, QueryRejection>,
    ) -> Result<Json<DocumentPage>, ApiError> {
        let Query(query) = query
            .map_err(|_| ApiError(PlatformError::Validation("invalid document catalog query")))?;
        state
            .run(move |store| store.project_document_page(&ctx, &project, &query))
            .await
            .map(Json)
    }
    pub async fn history(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        query: Result<Query<DocumentHistoryQuery>, QueryRejection>,
    ) -> Result<Json<DocumentHistoryPage>, ApiError> {
        let Query(query) = query
            .map_err(|_| ApiError(PlatformError::Validation("invalid document history query")))?;
        state
            .run(move |store| {
                store.document_history_page(
                    &ctx,
                    &project,
                    &id,
                    query.after,
                    query.limit.unwrap_or(50),
                )
            })
            .await
            .map(Json)
    }
    pub async fn publications(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        query: Result<Query<DocumentPublicationsQuery>, QueryRejection>,
    ) -> Result<Json<DocumentPublicationPage>, ApiError> {
        let Query(query) = query.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid document publication query",
            ))
        })?;
        state
            .run(move |store| {
                store.document_publication_page(
                    &ctx,
                    &project,
                    &id,
                    query.after.as_deref(),
                    query.limit.unwrap_or(50),
                )
            })
            .await
            .map(Json)
    }
}
