use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
    oidc_callback_query::OidcCallbackQuery, oidc_start_request::OidcStartRequest,
};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct OidcHandlers;
impl OidcHandlers {
    pub async fn providers(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Value>, ApiError> {
        let policy = state.identity_policy();
        state.run(move |store| {
            let items=store.oidc_providers(&ctx)?;
            let policy=store.installation_oidc_policy(&policy)?;
            let methods=store.authentication_policy()?;
            let can_disable=store.can_disable_local_login(&ctx)?;
            Ok(json!({"items":items,"allowed_provider_origins":policy.allowed_provider_origins,"allowed_callback_origins":policy.allowed_callback_origins,"callback_url":format!("{}/api/v1/auth/oidc/callback",policy.allowed_callback_origins[0]),"local_enabled":methods.local_enabled,"policy_revision":methods.revision,"can_disable_local":can_disable}))
        }).await.map(Json)
    }

    pub async fn change(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<OidcProviderChange>, JsonRejection>,
    ) -> Result<MutationResponse<OidcProvider>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid corporate provider change",
            ))
        })?;
        let policy = state.identity_policy();
        let request_id = MutationId::required(&headers)?;
        let canonical = json!({"id":id,"change":input});
        let provider = state
            .mutate(
                ctx,
                "oidc.change",
                request_id.clone(),
                canonical,
                move |store, ctx| store.change_oidc_provider(ctx, &id, &input, &policy),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(provider),
        })
    }
    pub async fn configure(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<OidcProviderRegistration>, JsonRejection>,
    ) -> Result<MutationResponse<OidcProvider>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid corporate provider request",
            ))
        })?;
        let status = if input.expected_revision == 0 {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        };
        let policy = state.identity_policy();
        let request_id = MutationId::read(&headers)?;
        let canonical =
            serde_json::to_value(&input).map_err(|_| ApiError(PlatformError::Storage))?;
        let provider = state
            .mutate(
                ctx,
                "oidc.configure",
                request_id.clone(),
                canonical,
                move |store, ctx| store.configure_oidc(ctx, &input, &policy),
            )
            .await?;
        Ok(MutationResponse {
            status,
            request_id,
            body: Some(provider),
        })
    }
    pub async fn methods(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
        state
            .run(|store| store.authentication_options())
            .await
            .map(Json)
    }
    pub async fn change_methods(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<AuthenticationPolicyChange>, JsonRejection>,
    ) -> Result<MutationResponse<AuthenticationPolicy>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid authentication policy")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical =
            serde_json::to_value(&input).map_err(|_| ApiError(PlatformError::Storage))?;
        let policy = state
            .mutate(
                ctx,
                "auth.policy",
                request_id.clone(),
                canonical,
                move |store, ctx| store.change_authentication_policy(ctx, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(policy),
        })
    }
    pub async fn create_user(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<OidcUserRegistration>, JsonRejection>,
    ) -> Result<MutationResponse<Actor>, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid corporate user request")))?;
        let request_id = MutationId::read(&headers)?;
        let canonical =
            serde_json::to_value(&input).map_err(|_| ApiError(PlatformError::Storage))?;
        let actor = state
            .mutate(
                ctx,
                "user.oidc.create",
                request_id.clone(),
                canonical,
                move |store, ctx| store.create_oidc_user(ctx, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(actor),
        })
    }
    pub async fn assign(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor_id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<OidcIdentityAssignment>, JsonRejection>,
    ) -> Result<MutationResponse<()>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid corporate identity mapping",
            ))
        })?;
        let request_id = MutationId::read(&headers)?;
        let canonical = json!({"actor_id":actor_id,"identity":input});
        state
            .mutate(
                ctx,
                "identity.oidc.assign",
                request_id.clone(),
                canonical,
                move |store, ctx| store.assign_oidc_identity(ctx, &actor_id, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::NO_CONTENT,
            request_id,
            body: None,
        })
    }
    pub async fn start(
        State(state): State<ApiState>,
        Path(provider_id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<OidcStartRequest>, JsonRejection>,
    ) -> Result<Response, ApiError> {
        let _ = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "OIDC start expects an empty object",
            ))
        })?;
        let browser_origin = crate::browser_session::BrowserSession::origin(&headers)?;
        let policy = state.identity_policy();
        let started = state
            .run(move |store| store.start_oidc(&provider_id, &policy))
            .await?;
        let cookie = format!(
            "nexora_oidc_{}={}; HttpOnly; SameSite=Lax; Path=/api/v1/auth/oidc/callback; Max-Age=600{}",
            started.state_id,
            started.correlation_secret,
            if started.secure_cookie {
                "; Secure"
            } else {
                ""
            }
        );
        let mut response = Json(&started).into_response();
        response.headers_mut().insert(
            "Set-Cookie",
            HeaderValue::from_str(&cookie).map_err(|_| ApiError(PlatformError::Storage))?,
        );
        if let Some(origin) = browser_origin {
            response.headers_mut().append("set-cookie",HeaderValue::from_str(&format!("nexora_oidc_browser_{}={}; HttpOnly; SameSite=Lax; Path=/api/v1/auth/oidc/callback; Max-Age=600{}",started.state_id,origin,if started.secure_cookie { "; Secure" } else { "" })).map_err(|_|ApiError(PlatformError::Storage))?);
        }
        Ok(response)
    }
    pub async fn callback(
        State(state): State<ApiState>,
        headers: HeaderMap,
        query: Result<Query<OidcCallbackQuery>, QueryRejection>,
    ) -> Response {
        let Ok(Query(input)) = query else {
            return ApiError(PlatformError::Unauthorized).into_response();
        };
        let Some(state_token) = input.state else {
            return ApiError(PlatformError::Unauthorized).into_response();
        };
        let Some((id, _)) = state_token.split_once('.') else {
            return ApiError(PlatformError::Unauthorized).into_response();
        };
        if id.len() != 36
            || !id.starts_with("oas_")
            || !id[4..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return ApiError(PlatformError::Unauthorized).into_response();
        }
        let cookie_name = format!("nexora_oidc_{id}");
        let matches: Vec<_> = headers
            .get_all("cookie")
            .iter()
            .filter_map(|h| h.to_str().ok())
            .flat_map(|s| s.split(';'))
            .filter_map(|s| s.trim().split_once('='))
            .filter(|(key, _)| *key == cookie_name)
            .map(|(_, value)| value.to_owned())
            .collect();
        if matches.len() != 1 {
            return ApiError(PlatformError::Unauthorized).into_response();
        }
        let correlation = matches.into_iter().next().unwrap();
        let policy = state.identity_policy();
        let code = if input.error.is_some() {
            String::new()
        } else {
            input.code.unwrap_or_default()
        };
        let browser_name = format!("nexora_oidc_browser_{id}");
        let result = state
            .run(move |store| store.complete_oidc(&state_token, &code, &correlation, &policy))
            .await;
        let browser_values: Vec<_> = headers
            .get_all("cookie")
            .iter()
            .filter_map(|h| h.to_str().ok())
            .flat_map(|s| s.split(';'))
            .filter_map(|s| s.trim().split_once('='))
            .filter(|(key, _)| *key == browser_name)
            .map(|(_, v)| v.to_owned())
            .collect();
        let mut browser_headers = HeaderMap::new();
        if browser_values.len() == 1
            && let Ok(origin) = HeaderValue::from_str(&browser_values[0])
        {
            browser_headers.insert("x-nexora-browser", origin);
        }
        let mut response = match result {
            Ok(session) => {
                match crate::browser_session::BrowserSession::issued(session, &browser_headers) {
                    Ok(response) => response,
                    Err(error) => error.into_response(),
                }
            }
            Err(error) => error.into_response(),
        };
        if let Ok(value) = HeaderValue::from_str(&format!(
            "{cookie_name}=; HttpOnly; SameSite=Lax; Path=/api/v1/auth/oidc/callback; Max-Age=0"
        )) {
            response.headers_mut().append("Set-Cookie", value);
        }
        if let Ok(value) = HeaderValue::from_str(&format!(
            "{browser_name}=; HttpOnly; SameSite=Lax; Path=/api/v1/auth/oidc/callback; Max-Age=0"
        )) {
            response.headers_mut().append("set-cookie", value);
        }
        response
    }
}
