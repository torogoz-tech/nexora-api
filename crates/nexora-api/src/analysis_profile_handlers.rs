use crate::{
    ApiState, analysis_profile_query::AnalysisProfileQuery, api_error::ApiError,
    mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_language::AnalysisProfilePackage;
use nexora_platform::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(crate) struct AnalysisProfileHandlers;
impl AnalysisProfileHandlers {
    pub async fn list(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(q): Query<AnalysisProfileQuery>,
    ) -> Result<Json<Value>, ApiError> {
        s.run(move |store| store.analysis_profiles(&ctx, q.after.as_deref(), q.limit))
            .await
            .map(Json)
    }
    pub async fn get(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        s.run(move |store| store.analysis_profile(&ctx, &id, false))
            .await
            .map(Json)
    }
    pub async fn export(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        s.run(move |store| store.analysis_profile(&ctx, &id, true))
            .await
            .map(Json)
    }
    pub async fn import(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<AnalysisProfilePackage>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid analysis profile package",
            ))
        })?;
        let id = MutationId::required(&headers)?;
        let bytes =
            serde_json::to_vec(&input.canonical()).map_err(|_| ApiError(PlatformError::Storage))?;
        if bytes.len() > AnalysisProfilePackage::MAX_BYTES {
            return Err(ApiError(PlatformError::Validation(
                "analysis profile exceeds 16 MiB",
            )));
        }
        let hash = format!("{:x}", Sha256::digest(bytes));
        let value = s
            .mutate(
                ctx,
                "analysis_profile.import",
                id.clone(),
                json!({"package_sha256":hash}),
                move |store, ctx| store.import_analysis_profile(ctx, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id: id,
            body: Some(value),
        })
    }
    pub async fn assignment(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((p, r)): Path<(String, String)>,
    ) -> Result<Json<Value>, ApiError> {
        s.run(move |store| store.repository_analysis_profile(&ctx, &p, &r))
            .await
            .map(Json)
    }
    pub async fn change(
        State(s): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((p, r)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<RepositoryAnalysisProfileChange>, JsonRejection>,
    ) -> Result<MutationResponse<Value>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid repository analysis profile",
            ))
        })?;
        let id = MutationId::required(&headers)?;
        let fingerprint = json!({"project_id":p,"repository_id":r,"change":input});
        let value = s
            .mutate(
                ctx,
                "repository.analysis_profile.change",
                id.clone(),
                fingerprint,
                move |store, ctx| store.change_repository_analysis_profile(ctx, &p, &r, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(value),
        })
    }
}
