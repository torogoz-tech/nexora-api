use crate::api_error::ApiError;
use axum::{
    Json,
    http::{HeaderMap, HeaderValue, Method, Uri},
    response::{IntoResponse, Response},
};
use nexora_platform::{IssuedSession, PlatformError};
use serde_json::{Value, json};
/// Browser credentials stay in host-only HttpOnly cookies. Bearer clients remain supported.
pub(crate) struct BrowserSession;
impl BrowserSession {
    pub fn origin(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
        let Some(value) = headers.get("x-nexora-browser") else {
            return Ok(None);
        };
        let value = value
            .to_str()
            .map_err(|_| ApiError(PlatformError::Forbidden))?;
        let uri: Uri = value
            .parse()
            .map_err(|_| ApiError(PlatformError::Forbidden))?;
        let local = matches!(uri.host(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if value
            .bytes()
            .any(|b| matches!(b, b';' | b',' | b' ' | b'\\' | b'\"'))
            || uri.authority().is_none()
            || uri.authority().is_some_and(|a| a.as_str().contains('@'))
            || uri.path_and_query().is_some_and(|p| p.as_str() != "/")
            || !(uri.scheme_str() == Some("https") || local && uri.scheme_str() == Some("http"))
            || headers
                .get("origin")
                .is_some_and(|origin| origin.as_bytes() != value.as_bytes())
        {
            return Err(ApiError(PlatformError::Forbidden));
        }
        Ok(Some(value.to_owned()))
    }
    pub fn cookie(headers: &HeaderMap) -> Result<String, ApiError> {
        let local = Self::origin(headers)?.is_some_and(|v| v.starts_with("http://"));
        let name = if local {
            "nexora_dev_session"
        } else {
            "__Host-nexora_session"
        };
        let values: Vec<_> = headers
            .get_all("cookie")
            .iter()
            .filter_map(|h| h.to_str().ok())
            .flat_map(|s| s.split(';'))
            .filter_map(|s| s.trim().split_once('='))
            .filter(|(key, _)| *key == name)
            .map(|(_, value)| value)
            .collect();
        if values.len() != 1 || !values[0].starts_with("nxs_") || values[0].len() > 256 {
            return Err(ApiError(PlatformError::Unauthorized));
        }
        Ok(values[0].to_owned())
    }
    pub fn token(headers: &HeaderMap) -> Result<String, ApiError> {
        if let Some(value) = headers.get("authorization") {
            return value
                .to_str()
                .ok()
                .and_then(|v| v.strip_prefix("Bearer "))
                .filter(|v| !v.is_empty() && v.len() <= 256)
                .map(str::to_owned)
                .ok_or(ApiError(PlatformError::Unauthorized));
        }
        Self::cookie(headers)
    }
    pub fn protect(headers: &HeaderMap, method: &Method) -> Result<(), ApiError> {
        if !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
            && Self::origin(headers)?.is_none()
        {
            return Err(ApiError(PlatformError::Forbidden));
        }
        Ok(())
    }
    pub fn header(origin: &str, token: &str, max_age: i64) -> Result<HeaderValue, ApiError> {
        let local = origin.starts_with("http://");
        let name = if local {
            "nexora_dev_session"
        } else {
            "__Host-nexora_session"
        };
        HeaderValue::from_str(&format!(
            "{name}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age}{}",
            if local { "" } else { "; Secure" }
        ))
        .map_err(|_| ApiError(PlatformError::Storage))
    }
    pub fn issued(session: IssuedSession, headers: &HeaderMap) -> Result<Response, ApiError> {
        let Some(origin) = Self::origin(headers)? else {
            return Ok(Json(session).into_response());
        };
        let expires = if session.next_step.is_some() {
            session.expires_in.min(900)
        } else {
            session.expires_in
        };
        let metadata = json!({"browser_session":true,"actor_id":session.actor_id,"session_id":session.access_token.strip_prefix("nxs_").and_then(|s|s.split_once('.')).map(|v|v.0),"expires_in":expires,"next_step":session.next_step});
        let mut response = Json(metadata).into_response();
        response.headers_mut().append(
            "set-cookie",
            Self::header(&origin, &session.access_token, expires)?,
        );
        Ok(response)
    }
    pub fn gate(mut value: Value, headers: &HeaderMap) -> Result<Response, ApiError> {
        let Some(origin) = Self::origin(headers)? else {
            return Ok(Json(value).into_response());
        };
        let cookie = if let Some(token) = value.get("access_token").and_then(Value::as_str) {
            value["session_id"] = json!(
                token
                    .strip_prefix("nxs_")
                    .and_then(|s| s.split_once('.'))
                    .map(|v| v.0)
            );
            Some(Self::header(
                &origin,
                value["access_token"].as_str().unwrap(),
                value["expires_in"].as_i64().unwrap_or(28800),
            )?)
        } else {
            None
        };
        if let Some(v) = value.as_object_mut() {
            v.remove("access_token");
            v.remove("token_type");
        }
        value["browser_session"] = json!(true);
        let mut response = Json(value).into_response();
        if let Some(cookie) = cookie {
            response.headers_mut().append("set-cookie", cookie);
        }
        Ok(response)
    }
}
