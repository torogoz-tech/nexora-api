use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use crate::{organization_logo::OrganizationLogo, organization_logo_input::OrganizationLogoInput};
use axum::response::IntoResponse;
use axum::{
    Extension, Json,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::json;
use sha2::{Digest, Sha256};
pub(crate) struct OrganizationSettingsHandlers;
impl OrganizationSettingsHandlers {
    pub async fn organization(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<OrganizationProfile>, ApiError> {
        state
            .run(move |s| s.organization_profile(&ctx))
            .await
            .map(Json)
    }
    pub async fn branding(
        State(state): State<ApiState>,
    ) -> Result<Json<OrganizationBranding>, ApiError> {
        state
            .run(|s| s.public_organization_branding())
            .await
            .map(Json)
    }
    pub async fn logo(State(state): State<ApiState>) -> Result<impl IntoResponse, ApiError> {
        let bytes = state.run(|s| s.organization_logo()).await?;
        Ok((
            [
                ("content-type", "image/png"),
                ("x-content-type-options", "nosniff"),
                ("cache-control", "no-store"),
            ],
            bytes,
        ))
    }
    pub async fn change_organization(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<OrganizationProfileChange>, JsonRejection>,
    ) -> Result<MutationResponse<OrganizationProfile>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid organization profile")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "organization.profile.change",
                id.clone(),
                json!(input),
                move |s, c| s.change_organization_profile(c, &input, None),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
    pub async fn change_logo(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<OrganizationLogoInput>, JsonRejection>,
    ) -> Result<MutationResponse<OrganizationProfile>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid logo request")))?;
        let id = MutationId::required(&headers)?;
        let fingerprint = json!({"revision":input.expected_revision,"logo_digest":input.data_base64.as_ref().map(|v|format!("{:x}",Sha256::digest(v.as_bytes()))) });
        let result = state
            .mutate(
                ctx,
                "organization.logo.change",
                id.clone(),
                fingerprint,
                move |s, c| {
                    // Check current authority before spending work decoding an image.
                    s.organization_profile(c)?;
                    let bytes = input
                        .data_base64
                        .as_deref()
                        .map(OrganizationLogo::normalize)
                        .transpose()?;
                    s.change_organization_profile(
                        c,
                        &OrganizationProfileChange {
                            expected_revision: input.expected_revision,
                            name: None,
                            address: None,
                            phone: None,
                            country_code: None,
                        },
                        Some(bytes),
                    )
                },
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
}
