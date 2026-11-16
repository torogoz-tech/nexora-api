use crate::{ApiState, api_error::ApiError, mutation_id::MutationId};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use nexora_platform::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub(crate) struct KnowledgeReviewHandlers;
impl KnowledgeReviewHandlers {
    pub async fn list(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<BTreeMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let resource = q.get("resource_id").cloned();
        let status = q.get("status").cloned().unwrap_or_else(|| "pending".into());
        let page = q
            .get("page")
            .map(|v| v.parse())
            .transpose()
            .map_err(|_| ApiError(PlatformError::Validation("invalid page")))?
            .unwrap_or(1);
        Ok(Json(
            s.run(move |store| store.knowledge_reviews(&ctx, resource.as_deref(), &status, page))
                .await?,
        ))
    }
    pub async fn get(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(
            s.run(move |store| store.knowledge_review(&ctx, &id))
                .await?,
        ))
    }
    pub async fn request(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        h: HeaderMap,
        Json(v): Json<Value>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(
            s.mutate(
                ctx,
                "knowledge.review.request",
                MutationId::required(&h)?,
                v.clone(),
                move |store, c| store.request_knowledge_review(c, &v),
            )
            .await?,
        ))
    }
    pub async fn decide(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        h: HeaderMap,
        Json(v): Json<Value>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(
            s.mutate(
                ctx,
                "knowledge.review.decide",
                MutationId::required(&h)?,
                json!({"id":id,"input":v}),
                move |store, c| store.decide_knowledge_review(c, &id, &v),
            )
            .await?,
        ))
    }
}
