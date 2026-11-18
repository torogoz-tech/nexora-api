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
    fn callback_failure(status: StatusCode) -> Response {
        let body = r#"<!doctype html><html lang="es"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Acceso · Nexora</title><style>
*{box-sizing:border-box}body{margin:0;min-height:100vh;display:grid;place-items:center;background:#08090b;color:#f4f4f5;font:16px/1.5 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;padding:24px}.card{width:min(100%,440px);padding:36px;border:1px solid #292b30;border-radius:24px;background:linear-gradient(145deg,#141518,#0c0d0f);box-shadow:0 24px 80px #0008}.brand{display:flex;align-items:center;gap:12px;margin-bottom:36px}.mark{width:42px;height:42px;border:1px solid #45474d;border-radius:14px;background:#24262a;display:grid;place-items:center;font-weight:700;letter-spacing:-.08em}.brand span{font-weight:650}.icon{width:52px;height:52px;border:1px solid #73393c;border-radius:18px;color:#ff9298;background:#351d20;display:grid;place-items:center;font-size:25px}h1{font-size:22px;line-height:1.25;margin:20px 0 8px}p{color:#a1a1aa;margin:0 0 26px}.button{display:inline-flex;justify-content:center;align-items:center;min-height:44px;padding:0 18px;border-radius:14px;background:#e4e4e7;color:#171719;text-decoration:none;font-weight:650}.hint{margin-top:24px!important;margin-bottom:0;font-size:12px;color:#71717a}@media(prefers-reduced-motion:no-preference){.card{animation:appear .35s ease-out}@keyframes appear{from{opacity:0;transform:translateY(8px)}to{opacity:1;transform:translateY(0)}}}</style></head><body><main class="card"><div class="brand"><div class="mark" aria-hidden="true">N</div><span>Nexora</span></div><div class="icon" aria-hidden="true">!</div><h1>No se pudo completar el acceso</h1><p>El enlace de inicio de sesión expiró, ya fue utilizado o no pertenece a esta ventana. Regresa a Nexora e inicia sesión de nuevo.</p><a class="button" href="/login">Volver al inicio de sesión</a><p class="hint">Puedes cerrar esta ventana después de regresar a la aplicación.</p></main></body></html>"#;
        let mut response = (status, axum::response::Html(body)).into_response();
        response
            .headers_mut()
            .insert("cache-control", HeaderValue::from_static("no-store"));
        response
    }

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
            return Self::callback_failure(StatusCode::UNAUTHORIZED);
        };
        let Some(state_token) = input.state else {
            return Self::callback_failure(StatusCode::UNAUTHORIZED);
        };
        let Some((id, _)) = state_token.split_once('.') else {
            return Self::callback_failure(StatusCode::UNAUTHORIZED);
        };
        if id.len() != 36
            || !id.starts_with("oas_")
            || !id[4..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Self::callback_failure(StatusCode::UNAUTHORIZED);
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
            Err(error) => {
                let response = error.into_response();
                Self::callback_failure(response.status())
            }
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
