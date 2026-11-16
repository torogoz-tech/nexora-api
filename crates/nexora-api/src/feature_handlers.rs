use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct FeatureHandlers;
impl FeatureHandlers {
    pub async fn proposal(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(
            state
                .run(move |store| store.feature_proposal(&ctx, &project, &id))
                .await?,
        ))
    }

    pub async fn complete(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<FeatureComplete>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid feature completion")))?;
        let key = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"feature_id":id,"completion":input});
        let value = state
            .mutate(
                ctx,
                "feature.complete",
                key.clone(),
                canonical,
                move |store, ctx| store.complete_feature(ctx, &project, &id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: key,
            body: Some(value),
        })
    }
    pub async fn catalog(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.feature_catalog(&ctx, &project))
            .await
            .map(Json)
    }
    pub async fn propose(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(project): Path<String>,
        headers: HeaderMap,
        body: Result<Json<FeatureProposalCreate>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid feature proposal")))?;
        let key = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"proposal":input});
        let value = state
            .mutate(
                ctx,
                "feature_map.propose",
                key.clone(),
                canonical,
                move |store, ctx| store.propose_feature_map(ctx, &project, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id: key,
            body: Some(value),
        })
    }
    pub async fn decide(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, id)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<FeatureProposalDecision>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid feature decision")))?;
        let key = MutationId::required(&headers)?;
        let canonical = json!({"project_id":project,"id":id,"decision":input});
        let value = state
            .mutate(
                ctx,
                "feature_map.decide",
                key.clone(),
                canonical,
                move |store, ctx| store.decide_feature_map(ctx, &project, &id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: key,
            body: Some(value),
        })
    }
}
