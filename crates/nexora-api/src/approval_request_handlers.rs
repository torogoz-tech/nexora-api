use crate::{
    ApiState, api_error::ApiError, approval_request_page_query::ApprovalRequestPageQuery,
    mutation_id::MutationId, mutation_response::MutationResponse,
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
pub(crate) struct ApprovalRequestHandlers;
impl ApprovalRequestHandlers {
    pub async fn create(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project_id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<ApprovalRequestCreate>, JsonRejection>,
    ) -> Result<MutationResponse<ApprovalRequest>, ApiError> {
        let Json(definition) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid approval request")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"definition":definition});
        let body = state
            .mutate(
                ctx,
                "approval_request.create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_approval_request(ctx, &project_id, &definition),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn decide(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, approval_request_id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<ApprovalRequestDecide>, JsonRejection>,
    ) -> Result<MutationResponse<ApprovalRequest>, ApiError> {
        let Json(decision) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid approval decision")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"approval_request_id":approval_request_id,"decision":decision});
        let body = state
            .mutate(
                ctx,
                "approval_request.decide",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.decide_approval_request(ctx, &project_id, &approval_request_id, &decision)
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(body),
        })
    }
    pub async fn cancel(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, approval_request_id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<ApprovalRequestCancel>, JsonRejection>,
    ) -> Result<MutationResponse<ApprovalRequest>, ApiError> {
        let Json(cancellation) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid approval cancellation")))?;
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project_id,"approval_request_id":approval_request_id,"cancellation":cancellation});
        let body = state
            .mutate(
                ctx,
                "approval_request.cancel",
                request_id.clone(),
                canonical,
                move |store, ctx| {
                    store.cancel_approval_request(
                        ctx,
                        &project_id,
                        &approval_request_id,
                        &cancellation,
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
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project_id, approval_request_id)): Path<(String, String)>,
    ) -> Result<Json<ApprovalRequest>, ApiError> {
        state
            .run(move |store| store.get_approval_request(&ctx, &project_id, &approval_request_id))
            .await
            .map(Json)
    }
    pub async fn page(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project_id): Path<String>,
        query: Result<Query<ApprovalRequestPageQuery>, QueryRejection>,
    ) -> Result<Json<ApprovalRequestPage>, ApiError> {
        let Query(query) = query
            .map_err(|_| ApiError(PlatformError::Validation("invalid approval page query")))?;
        state
            .run(move |store| {
                store.approval_request_page(
                    &ctx,
                    &project_id,
                    query.after.as_deref(),
                    query.limit.unwrap_or(50),
                    query.status,
                )
            })
            .await
            .map(Json)
    }
}
