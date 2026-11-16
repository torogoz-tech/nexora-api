use crate::{ApiState, api_error::ApiError, git_callback_query::GitCallbackQuery};
use axum::{
    Extension, Json,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
};
use nexora_platform::*;

pub(crate) struct GithubManifestHandlers;
impl GithubManifestHandlers {
    pub async fn start(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        body: Result<Json<GithubManifestRequest>, JsonRejection>,
    ) -> Result<Response, ApiError> {
        let Json(input) = body
            .map_err(|_| ApiError(PlatformError::Validation("invalid GitHub manifest request")))?;
        let policy = state.git_policy();
        let flow = state
            .run(move |s| s.start_github_manifest(&ctx, &input, &policy))
            .await?;
        let cookie = format!(
            "nexora_manifest_{}={}; HttpOnly; SameSite=Lax; Path=/api/v1/git/github/manifest/callback; Max-Age=600{}",
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
            || !id.starts_with("gms_")
            || !id[4..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return ApiError(PlatformError::Unauthorized).into_response();
        }
        let name = format!("nexora_manifest_{id}");
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
            .run(move |s| s.complete_github_manifest(&token, &code, &cookie, &policy))
            .await
        {
            Ok(connection) => Json(connection).into_response(),
            Err(error) => error.into_response(),
        };
        if let Ok(value) = HeaderValue::from_str(&format!(
            "{name}=; HttpOnly; SameSite=Lax; Path=/api/v1/git/github/manifest/callback; Max-Age=0"
        )) {
            response.headers_mut().insert("Set-Cookie", value);
        }
        response
    }
}
