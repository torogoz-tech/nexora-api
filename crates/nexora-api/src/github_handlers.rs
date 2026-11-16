use crate::{
    ApiState, api_error::ApiError, git_callback_query::GitCallbackQuery,
    git_start_request::GitStartRequest, mutation_id::MutationId,
    mutation_response::MutationResponse,
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
use serde_json::json;
pub(crate) struct GithubHandlers;
impl GithubHandlers {
    pub async fn webhook_health(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<serde_json::Value>, ApiError> {
        state
            .run(move |store| store.github_webhook_health(&ctx))
            .await
            .map(Json)
    }
    pub async fn check_webhook(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<serde_json::Value>, ApiError> {
        let policy = state.git_policy();
        state
            .run(move |store| store.check_github_webhook(&ctx, &policy))
            .await
            .map(Json)
    }

    pub async fn configuration(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<GithubConfiguration>, ApiError> {
        Ok(Json(
            state.run(move |s| s.github_configuration(&ctx)).await?,
        ))
    }
    pub async fn change_state(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<GitConfigurationStateChange>, JsonRejection>,
    ) -> Result<MutationResponse<GithubConfiguration>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid provider state change")))?;
        let policy = state.git_policy();
        let request_id = MutationId::read(&headers)?;
        let data = serde_json::to_value(&input).map_err(|_| ApiError(PlatformError::Storage))?;
        let result = state
            .mutate(
                ctx,
                "github.configuration.state",
                request_id.clone(),
                data,
                move |store, ctx| store.change_github_configuration_state(ctx, &input, &policy),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id,
            body: Some(result),
        })
    }
    pub async fn configure(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        headers: HeaderMap,
        body: Result<Json<GithubAppRegistration>, JsonRejection>,
    ) -> Result<MutationResponse<GithubConfiguration>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid GitHub configuration")))?;
        let status = if input.expected_revision == 0 {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        };
        let policy = state.git_policy();
        let request_id = MutationId::read(&headers)?;
        let data = serde_json::to_value(&input).map_err(|_| ApiError(PlatformError::Storage))?;
        let config = state
            .mutate(
                ctx,
                "github.configure",
                request_id.clone(),
                data,
                move |s, c| {
                    let config = s.configure_github(c, &input, &policy)?;
                    if config.enabled {
                        let _ = s.check_github_webhook(c, &policy);
                    }
                    Ok(config)
                },
            )
            .await?;
        Ok(MutationResponse {
            status,
            request_id,
            body: Some(config),
        })
    }
    pub async fn start(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        body: Result<Json<GitStartRequest>, JsonRejection>,
    ) -> Result<Response, ApiError> {
        let _ = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "Git start expects an empty object",
            ))
        })?;
        let policy = state.git_policy();
        let flow = state.run(move |s| s.start_github(&ctx, &policy)).await?;
        let cookie = format!(
            "nexora_git_{}={}; HttpOnly; SameSite=Lax; Path=/api/v1/git/github/callback; Max-Age=600{}",
            flow.state_id,
            flow.correlation_secret,
            if flow.secure_cookie { "; Secure" } else { "" }
        );
        let mut response = Json(flow).into_response();
        response.headers_mut().insert(
            "Set-Cookie",
            HeaderValue::from_str(&cookie).map_err(|_| ApiError(PlatformError::Storage))?,
        );
        Ok(response)
    }
    pub async fn callback(
        State(state): State<ApiState>,
        headers: HeaderMap,
        query: Result<Query<GitCallbackQuery>, QueryRejection>,
    ) -> Response {
        let Ok(Query(input)) = query else {
            return ApiError(PlatformError::Unauthorized).into_response();
        };
        let Some(token) = input.state else {
            return ApiError(PlatformError::Unauthorized).into_response();
        };
        let Some((id, _)) = token.split_once('.') else {
            return ApiError(PlatformError::Unauthorized).into_response();
        };
        if id.len() != 36
            || !id.starts_with("gsa_")
            || !id[4..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return ApiError(PlatformError::Unauthorized).into_response();
        }
        let name = format!("nexora_git_{id}");
        let values: Vec<_> = headers
            .get_all("cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|s| s.split(';'))
            .filter_map(|s| s.trim().split_once('='))
            .filter(|(k, _)| *k == name)
            .map(|(_, v)| v.to_owned())
            .collect();
        if values.len() != 1 {
            return ApiError(PlatformError::Unauthorized).into_response();
        }
        let cookie = values.into_iter().next().unwrap();
        let policy = state.git_policy();
        let code = if input.error.is_some() {
            String::new()
        } else {
            input.code.unwrap_or_default()
        };
        let mut response = match state
            .run(move |s| s.complete_github(&token, &code, &cookie, &policy))
            .await
        {
            Ok(connection) => Json(connection).into_response(),
            Err(error) => error.into_response(),
        };
        if let Ok(value) = HeaderValue::from_str(&format!(
            "{name}=; HttpOnly; SameSite=Lax; Path=/api/v1/git/github/callback; Max-Age=0"
        )) {
            response.headers_mut().insert("Set-Cookie", value);
        }
        response
    }
    pub async fn installations(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        query: Result<Query<crate::github_page_query::GithubPageQuery>, QueryRejection>,
    ) -> Result<Json<GithubInstallationPage>, ApiError> {
        let Query(page) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid provider page")))?;
        let policy = state.git_policy();
        Ok(Json(
            state
                .run(move |s| {
                    s.github_installations(
                        &ctx,
                        &id,
                        page.page.unwrap_or(1),
                        page.limit.unwrap_or(50),
                        &policy,
                    )
                })
                .await?,
        ))
    }
    pub async fn select(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Result<Json<GithubInstallationSelection>, JsonRejection>,
    ) -> Result<MutationResponse<GitConnection>, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid installation selection")))?;
        let request_id = MutationId::read(&headers)?;
        let policy = state.git_policy();
        let data = json!({"authorization_id":id,"selection":input});
        let connection = state
            .mutate(
                ctx,
                "github.connection.select",
                request_id.clone(),
                data,
                move |s, c| s.select_github_installation(c, &id, &input, &policy),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(connection),
        })
    }
}
