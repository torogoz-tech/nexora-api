use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::extract::Path;
use axum::{
    Extension, Json,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::json;
use sha2::{Digest, Sha256};
pub(crate) struct InstallationServiceHandlers;
impl InstallationServiceHandlers {
    pub async fn service(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(kind): Path<InstallationService>,
    ) -> Result<Json<InstallationServiceSettings>, ApiError> {
        state
            .run(move |s| s.installation_service_settings(&ctx, kind))
            .await
            .map(Json)
    }
    pub async fn change_service(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(kind): Path<InstallationService>,
        headers: HeaderMap,
        body: Result<Json<InstallationServiceChange>, JsonRejection>,
    ) -> Result<MutationResponse<InstallationServiceSettings>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid service configuration")))?;
        let id = MutationId::required(&headers)?;
        let fingerprint = json!({"kind":kind,"settings":input.settings,"expected_revision":input.expected_revision,"remove_secret":input.remove_secret,"secret_digest":input.secret.as_ref().map(|s|format!("{:x}",Sha256::digest(s.as_bytes()))) });
        let result = state
            .mutate(
                ctx,
                "installation.service.change",
                id.clone(),
                fingerprint,
                move |s, c| s.change_installation_service(c, kind, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
}
