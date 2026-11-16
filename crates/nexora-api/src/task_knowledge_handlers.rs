use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId,
    task_knowledge_query::TaskKnowledgeQuery,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use nexora_platform::{AuthContext, TaskKnowledgeLink};
use serde_json::{Value, json};
pub(crate) struct TaskKnowledgeHandlers;
impl TaskKnowledgeHandlers {
    pub async fn link(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        Json(input): Json<TaskKnowledgeLink>,
    ) -> Result<Json<Value>, ApiError> {
        let key = MutationId::required(&headers)?;
        let canonical = json!({"task_id":id,"link":input});
        state
            .mutate(
                ctx,
                if input.kind == "requirement" {
                    "document.link_create"
                } else {
                    "task.knowledge.link"
                },
                key,
                canonical,
                move |store, ctx| store.link_task_knowledge(ctx, &id, &input),
            )
            .await
            .map(Json)
    }
    pub async fn options(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        Query(q): Query<TaskKnowledgeQuery>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.task_knowledge_options(&ctx, &id, &q.kind, q.after.as_deref()))
            .await
            .map(Json)
    }
}
