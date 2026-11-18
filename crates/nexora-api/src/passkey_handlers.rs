use crate::{
    ApiState, api_error::ApiError, api_middleware::ClientAddress, browser_session::BrowserSession,
    security_method_request::SecurityMethodRequest,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, Uri},
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;

async fn ceremony_origin(
    state: &ApiState,
    headers: &HeaderMap,
) -> Result<(String, String), ApiError> {
    let origin = BrowserSession::origin(headers)?.ok_or(ApiError(PlatformError::Forbidden))?;
    let policy = state.identity_policy();
    let policy = state
        .run(move |store| store.installation_oidc_policy(&policy))
        .await?;
    if !policy.allowed_callback_origins.contains(&origin) {
        return Err(ApiError(PlatformError::Forbidden));
    }
    let uri: Uri = origin
        .parse()
        .map_err(|_| ApiError(PlatformError::Forbidden))?;
    let host = uri.host().ok_or(ApiError(PlatformError::Forbidden))?;
    Ok((host.to_owned(), origin))
}

fn session_key(token: &str) -> Result<&str, ApiError> {
    token
        .strip_prefix("nxs_")
        .and_then(|value| value.split_once('.'))
        .map(|(id, _)| id)
        .filter(|id| !id.is_empty())
        .ok_or(ApiError(PlatformError::Unauthorized))
}

async fn enforce_attempt_limit(
    state: &ApiState,
    principal: String,
    peer: Option<String>,
) -> Result<(), ApiError> {
    state
        .run(move |store| {
            store.consume_authentication_attempt(
                &format!("passkey:principal:{principal}"),
                20,
                900,
            )?;
            if let Some(peer) = peer {
                store.consume_authentication_attempt(&format!("passkey:peer:{peer}"), 120, 900)?;
            }
            Ok(())
        })
        .await
}

pub(crate) struct PasskeyHandlers;

impl PasskeyHandlers {
    pub async fn gate_start(
        State(state): State<ApiState>,
        headers: HeaderMap,
        Extension(peer): Extension<ClientAddress>,
    ) -> Result<Json<Value>, ApiError> {
        let token = BrowserSession::token(&headers)?;
        enforce_attempt_limit(&state, session_key(&token)?.to_owned(), peer.0).await?;
        let (rp_id, origin) = ceremony_origin(&state, &headers).await?;
        state
            .run(move |store| store.start_passkey_gate_authentication(&token, &rp_id, &origin))
            .await
            .map(Json)
    }

    pub async fn gate_finish(
        State(state): State<ApiState>,
        headers: HeaderMap,
        Extension(peer): Extension<ClientAddress>,
        Path(ceremony_id): Path<String>,
        Json(input): Json<Value>,
    ) -> Result<axum::response::Response, ApiError> {
        let token = BrowserSession::token(&headers)?;
        enforce_attempt_limit(&state, session_key(&token)?.to_owned(), peer.0).await?;
        let (rp_id, origin) = ceremony_origin(&state, &headers).await?;
        let response =
            input
                .get("response")
                .cloned()
                .ok_or(ApiError(PlatformError::Validation(
                    "passkey response is required",
                )))?;
        let session = state
            .run(move |store| {
                store.finish_passkey_gate_authentication(
                    &token,
                    &ceremony_id,
                    response,
                    &rp_id,
                    &origin,
                )
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
        Extension(peer): Extension<ClientAddress>,
    ) -> Result<Json<Value>, ApiError> {
        enforce_attempt_limit(&state, ctx.actor_id().to_owned(), peer.0).await?;
        let (rp_id, origin) = ceremony_origin(&state, &headers).await?;
        state
            .run(move |store| store.start_passkey_registration(&ctx, &rp_id, &origin))
            .await
            .map(Json)
    }

    pub async fn finish(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        Extension(peer): Extension<ClientAddress>,
        Path(ceremony_id): Path<String>,
        Json(input): Json<Value>,
    ) -> Result<Json<Value>, ApiError> {
        enforce_attempt_limit(&state, ctx.actor_id().to_owned(), peer.0).await?;
        let (rp_id, origin) = ceremony_origin(&state, &headers).await?;
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
