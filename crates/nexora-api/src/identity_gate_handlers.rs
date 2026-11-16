use crate::{
    ApiState, api_error::ApiError, identity_gate_request::IdentityGateRequest,
    totp_exemption_request::TotpExemptionRequest,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::AuthContext;
use serde_json::Value;
pub(crate) struct IdentityGateHandlers;
impl IdentityGateHandlers {
    pub async fn advance(
        State(state): State<ApiState>,
        headers: HeaderMap,
        Json(input): Json<IdentityGateRequest>,
    ) -> Result<axum::response::Response, ApiError> {
        let token = crate::browser_session::BrowserSession::token(&headers)?;
        if !headers.contains_key("authorization") {
            crate::browser_session::BrowserSession::protect(&headers, &axum::http::Method::POST)?;
        }
        let value = state
            .run(move |store| store.identity_gate(&token, &input.action, &input.code))
            .await?;
        crate::browser_session::BrowserSession::gate(value, &headers)
    }
    pub async fn status(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.member_totp(&ctx, &actor))
            .await
            .map(Json)
    }
    pub async fn exempt(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
        Json(input): Json<TotpExemptionRequest>,
    ) -> Result<StatusCode, ApiError> {
        state
            .run(move |store| store.set_totp_exemption(&ctx, &actor, input.exempt))
            .await?;
        Ok(StatusCode::NO_CONTENT)
    }
}
