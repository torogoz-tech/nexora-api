use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::json;

pub(crate) struct TaskApprovalHandlers;
impl TaskApprovalHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, task, id)): Path<(String, String, String)>,
    ) -> Result<Json<TaskApproval>, ApiError> {
        state
            .run(move |store| store.get_task_approval(&ctx, &project, &task, &id))
            .await
            .map(Json)
    }
    pub async fn revoke(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, task, id)): Path<(String, String, String)>,
        headers: HeaderMap,
        body: Result<Json<TaskApprovalRevoke>, JsonRejection>,
    ) -> Result<MutationResponse<TaskApproval>, ApiError> {
        let Json(revocation) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid approval revocation")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical =
            json!({"project_id":project,"task_id":task,"approval_id":id,"revocation":revocation});
        let body = state
            .mutate(
                ctx,
                "task.approval_revoke",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.revoke_task_approval(ctx, &project, &task, &id, &revocation)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
}
