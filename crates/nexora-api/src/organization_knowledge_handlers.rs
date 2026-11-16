use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId,
    organization_knowledge_query::OrganizationKnowledgeQuery,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use nexora_platform::{AuthContext, OrganizationDocumentWrite};
use serde_json::{Value, json};
pub(crate) struct OrganizationKnowledgeHandlers;
impl OrganizationKnowledgeHandlers {
    pub async fn page(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<OrganizationKnowledgeQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| {
                store.organization_knowledge_page(
                    &ctx,
                    q.kind.as_deref().unwrap_or("document"),
                    q.search.as_deref().unwrap_or(""),
                    q.after.as_deref(),
                )
            })
            .await
            .map(Json)
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        Query(q): Query<OrganizationKnowledgeQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.organization_knowledge_get(&ctx, &id, q.revision))
            .await
            .map(Json)
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        Json(input): Json<OrganizationDocumentWrite>,
    ) -> Result<Json<Value>, ApiError> {
        let request = MutationId::required(&headers)?;
        let canonical = json!({"document":input});
        state
            .mutate(
                ctx,
                "organization.document.write",
                request,
                canonical,
                move |store, ctx| store.organization_knowledge_write(ctx, None, &input),
            )
            .await
            .map(Json)
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        Json(input): Json<OrganizationDocumentWrite>,
    ) -> Result<Json<Value>, ApiError> {
        let request = MutationId::required(&headers)?;
        let canonical = json!({"id":id,"document":input});
        state
            .mutate(
                ctx,
                "organization.document.write",
                request,
                canonical,
                move |store, ctx| store.organization_knowledge_write(ctx, Some(&id), &input),
            )
            .await
            .map(Json)
    }
}
