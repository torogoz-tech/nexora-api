use crate::{
    ApiState, api_error::ApiError, browser_session::BrowserSession,
    security_method_request::SecurityMethodRequest,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, Uri},
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;

fn ceremony_origin(headers: &HeaderMap) -> Result<(String, String), ApiError> {
    let origin = BrowserSession::origin(headers)?.ok_or(ApiError(PlatformError::Forbidden))?;
    let uri: Uri = origin
        .parse()
        .map_err(|_| ApiError(PlatformError::Forbidden))?;
    let host = uri.host().ok_or(ApiError(PlatformError::Forbidden))?;
    Ok((host.to_owned(), origin))
}

pub(crate) struct PasskeyHandlers;

impl PasskeyHandlers {
    pub async fn authenticate_start(
        State(state): State<ApiState>,
        headers: HeaderMap,
        Json(input): Json<Value>,
    ) -> Result<Json<Value>, ApiError> {
        let (rp_id, origin) = ceremony_origin(&headers)?;
        let username = input
            .get("username")
            .and_then(Value::as_str)
            .ok_or(ApiError(PlatformError::Unauthorized))?
            .to_owned();
        state
            .run(move |store| store.start_passkey_authentication(&username, &rp_id, &origin))
            .await
            .map(Json)
    }

    pub async fn authenticate_finish(
        State(state): State<ApiState>,
        headers: HeaderMap,
        Path(ceremony_id): Path<String>,
        Json(input): Json<Value>,
    ) -> Result<axum::response::Response, ApiError> {
        let (rp_id, origin) = ceremony_origin(&headers)?;
        let response =
            input
                .get("response")
                .cloned()
                .ok_or(ApiError(PlatformError::Validation(
                    "passkey response is required",
                )))?;
        let session = state
            .run(move |store| {
                store.finish_passkey_authentication(&ceremony_id, response, &rp_id, &origin)
            })
            .await?;
        BrowserSession::issued(session, &headers)
    }

    pub async fn status(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.passkey_status(&ctx))
            .await
            .map(Json)
    }

    pub async fn set_method(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Json(input): Json<SecurityMethodRequest>,
    ) -> Result<Json<Value>, ApiError> {
        state
            .run(move |store| store.set_primary_auth_method(&ctx, &input.method))
            .await
            .map(Json)
    }

    pub async fn start(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
    ) -> Result<Json<Value>, ApiError> {
        let (rp_id, origin) = ceremony_origin(&headers)?;
        state
            .run(move |store| store.start_passkey_registration(&ctx, &rp_id, &origin))
            .await
            .map(Json)
    }

    pub async fn finish(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        Path(ceremony_id): Path<String>,
        Json(input): Json<Value>,
    ) -> Result<Json<Value>, ApiError> {
        let (rp_id, origin) = ceremony_origin(&headers)?;
        let response =
            input
                .get("response")
                .cloned()
                .ok_or(ApiError(PlatformError::Validation(
                    "passkey response is required",
                )))?;
        let label = input
            .get("label")
            .and_then(Value::as_str)
            .map(str::to_owned);
        state
            .run(move |store| {
                store.finish_passkey_registration(
                    &ctx,
                    &ceremony_id,
                    response,
                    label.as_deref(),
                    &rp_id,
                    &origin,
                )
            })
            .await
            .map(Json)
    }
}
