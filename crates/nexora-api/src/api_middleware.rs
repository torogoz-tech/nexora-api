use crate::{ApiState, api_error::ApiError};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use nexora_platform::PlatformError;
use nexora_platform_store::SecretGenerator;
pub(crate) struct ApiMiddleware;
impl ApiMiddleware {
    pub async fn authenticate(
        State(state): State<ApiState>,
        mut request: Request,
        next: Next,
    ) -> Result<Response, ApiError> {
        let cookie = !request.headers().contains_key("authorization");
        if cookie && !request.uri().path().starts_with("/api/") {
            return Err(ApiError(PlatformError::Unauthorized));
        }
        let token = crate::browser_session::BrowserSession::token(request.headers())?;
        if cookie {
            crate::browser_session::BrowserSession::protect(request.headers(), request.method())?;
            request.headers_mut().insert(
                "authorization",
                format!("Bearer {token}")
                    .parse()
                    .map_err(|_| ApiError(PlatformError::Unauthorized))?,
            );
        }
        let ctx = state.run(move |store| store.authenticate(&token)).await?;
        let path = request.uri().path().to_owned();
        let access = ctx.clone();
        state
            .run(move |store| store.authorize_request_workspace(&access, &path))
            .await?;
        request.extensions_mut().insert(ctx);
        Ok(next.run(request).await)
    }
    pub async fn request_id(request: Request, next: Next) -> Response {
        let id = SecretGenerator::id("req").ok();
        let mut response = next.run(request).await;
        if let Some(id) = id
            && let Ok(header) = id.parse()
        {
            response.headers_mut().insert("x-request-id", header);
        }
        let streaming = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"));
        let private = response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(',')
                    .any(|directive| directive.trim() == "private")
            });
        response.headers_mut().insert(
            "cache-control",
            axum::http::HeaderValue::from_static(if streaming {
                "no-store, no-transform"
            } else if private {
                "private, no-store"
            } else {
                "no-store"
            }),
        );
        response
    }
}
