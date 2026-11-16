use crate::{
    ApiState, api_error::ApiError, knowledge_contribution_query::KnowledgeContributionQuery,
    mutation_id::MutationId,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_platform::{AuthContext, KnowledgeContributionInput, PlatformError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(crate) struct KnowledgeContributionHandlers;
impl KnowledgeContributionHandlers {
    pub async fn write(
        state: ApiState,
        ctx: AuthContext,
        id: Option<String>,
        request: String,
        input: KnowledgeContributionInput,
    ) -> Result<Value, ApiError> {
        if input
            .data_base64
            .as_ref()
            .is_some_and(|s| s.len() > 6_990_508)
        {
            return Err(ApiError(PlatformError::Validation(
                "document exceeds 5 MiB",
            )));
        }
        let bytes = input
            .data_base64
            .as_ref()
            .map(|s| STANDARD.decode(s))
            .transpose()
            .map_err(|_| ApiError(PlatformError::Validation("invalid base64")))?;
        let mut metadata = input.clone();
        metadata.data_base64 = None;
        let canonical = json!({"id":id,"input":metadata,"sha256":bytes.as_ref().map(|v|format!("{:x}",Sha256::digest(v)))});
        state
            .mutate(
                ctx,
                "knowledge.contribute",
                request,
                canonical,
                move |store, ctx| {
                    store.write_knowledge_contribution(ctx, id.as_deref(), &input, bytes.as_deref())
                },
            )
            .await
    }
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        Json(input): Json<KnowledgeContributionInput>,
    ) -> Result<Json<Value>, ApiError> {
        Self::write(state, ctx, None, MutationId::required(&headers)?, input)
            .await
            .map(Json)
    }
    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        Json(input): Json<KnowledgeContributionInput>,
    ) -> Result<Json<Value>, ApiError> {
        Self::write(state, ctx, Some(id), MutationId::required(&headers)?, input)
            .await
            .map(Json)
    }
    pub async fn page(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<KnowledgeContributionQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| {
                store.knowledge_contributions(
                    &ctx,
                    q.project_id.as_deref(),
                    q.kind.as_deref().unwrap_or("document"),
                    q.search.as_deref().unwrap_or(""),
                    q.after.as_deref(),
                    q.descending.unwrap_or(false),
                )
            })
            .await
            .map(Json)
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        Query(q): Query<KnowledgeContributionQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.knowledge_contribution(&ctx, &id, q.revision))
            .await
            .map(Json)
    }
    pub async fn file(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        Query(q): Query<KnowledgeContributionQuery>,
    ) -> Result<axum::response::Response, ApiError> {
        let bytes = state
            .run(move |store| store.knowledge_contribution_file(&ctx, &id, q.revision))
            .await?;
        Ok((
            [
                ("content-type", "application/octet-stream"),
                ("content-disposition", "attachment"),
                ("x-content-type-options", "nosniff"),
                ("cache-control", "private, no-store"),
                ("content-security-policy", "sandbox"),
            ],
            bytes,
        )
            .into_response())
    }
}
