use crate::{ApiState, api_error::ApiError, browser_session::BrowserSession};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use nexora_platform::PlatformError;
pub(crate) struct BrowserSessionHandlers;
impl BrowserSessionHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        headers: HeaderMap,
    ) -> Result<Response, ApiError> {
        BrowserSession::origin(&headers)?.ok_or(ApiError(PlatformError::Forbidden))?;
        let token = BrowserSession::cookie(&headers)?;
        state
            .run(move |store| store.browser_session(&token))
            .await
            .map(|value| Json(value).into_response())
    }
    pub async fn adopt(
        State(state): State<ApiState>,
        headers: HeaderMap,
    ) -> Result<Response, ApiError> {
        let origin = BrowserSession::origin(&headers)?.ok_or(ApiError(PlatformError::Forbidden))?;
        if !headers.contains_key("authorization") {
            return Err(ApiError(PlatformError::Unauthorized));
        }
        let token = BrowserSession::token(&headers)?;
        let credential = token.clone();
        let metadata = state
            .run(move |store| store.browser_session(&credential))
            .await?;
        let expires = metadata["expires_in"]
            .as_i64()
            .ok_or(ApiError(PlatformError::Unauthorized))?;
        let mut response = Json(metadata).into_response();
        response.headers_mut().append(
            "set-cookie",
            BrowserSession::header(&origin, &token, expires)?,
        );
        Ok(response)
    }
    pub async fn delete(
        State(state): State<ApiState>,
        headers: HeaderMap,
    ) -> Result<Response, ApiError> {
        BrowserSession::protect(&headers, &Method::DELETE)?;
        let origin = BrowserSession::origin(&headers)?.ok_or(ApiError(PlatformError::Forbidden))?;
        if let Ok(token) = BrowserSession::cookie(&headers) {
            match state
                .run(move |store| store.end_browser_session(&token))
                .await
            {
                Ok(()) => {}
                Err(ApiError(PlatformError::Unauthorized)) => {}
                Err(error) => return Err(error),
            }
        }
        let mut response = StatusCode::NO_CONTENT.into_response();
        response
            .headers_mut()
            .append("set-cookie", BrowserSession::header(&origin, "", 0)?);
        Ok(response)
    }
}
