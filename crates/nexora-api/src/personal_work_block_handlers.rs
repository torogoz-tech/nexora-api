use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct PersonalWorkBlockHandlers;
impl PersonalWorkBlockHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<std::collections::BTreeMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let from = q.get("from").cloned().unwrap_or_default();
        let to = q.get("to").cloned().unwrap_or_default();
        state
            .run(move |s| s.personal_work_blocks(&ctx, &from, &to))
            .await
            .map(Json)
    }
    pub async fn create(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        h: HeaderMap,
        Json(v): Json<Value>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        Self::save(s, ctx, None, h, v, false).await
    }
    pub async fn change(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        h: HeaderMap,
        Json(v): Json<Value>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        Self::save(s, ctx, Some(id), h, v, false).await
    }
    pub async fn remove(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        h: HeaderMap,
        Json(v): Json<Value>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        Self::save(s, ctx, Some(id), h, v, true).await
    }
    async fn save(
        s: ApiState,
        ctx: AuthContext,
        id: Option<String>,
        h: HeaderMap,
        v: Value,
        remove: bool,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let request_id = MutationId::required(&h)?;
        let op = if remove {
            "calendar.work_block.remove"
        } else if id.is_some() {
            "calendar.work_block.change"
        } else {
            "calendar.work_block.create"
        };
        let canonical = json!({"id":id,"input":v,"remove":remove});
        let body = s
            .mutate(ctx, op, request_id.clone(), canonical, move |s, ctx| {
                s.save_personal_work_block(ctx, id.as_deref(), &v, remove)
            })
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
