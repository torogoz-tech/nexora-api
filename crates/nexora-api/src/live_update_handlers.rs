use crate::{ApiState, api_error::ApiError, live_update_query::LiveUpdateQuery};
use axum::{
    extract::{Query, State, rejection::QueryRejection},
    http::HeaderMap,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
};
use futures_util::{StreamExt, stream};
use nexora_platform::PlatformError;
use serde_json::json;
use std::{convert::Infallible, time::Duration};
pub(crate) struct LiveUpdateHandlers;
impl LiveUpdateHandlers {
    pub async fn subscribe(
        State(state): State<ApiState>,
        query: Result<Query<LiveUpdateQuery>, QueryRejection>,
        headers: HeaderMap,
    ) -> Result<Response, ApiError> {
        let Query(input) =
            query.map_err(|_| ApiError(PlatformError::Validation("invalid live update query")))?;
        let token = zeroize::Zeroizing::new(
            headers
                .get("authorization")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .ok_or(ApiError(PlatformError::Unauthorized))?
                .to_string(),
        );
        let permit = state.live_update_permit()?;
        let initial = Self::snapshot(&state, &token, input.project_id.clone()).await?;
        // A stream does not occupy a DB connection or worker permit between snapshots.
        // Every connection starts with full invalidation; reconnect cannot miss committed changes.
        let events = stream::unfold(
            (state, token, input.project_id, initial, true, Some(())),
            |(state, token, project, previous, first, alive)| async move {
                alive?;
                if first {
                    let topics = previous.keys().collect::<Vec<_>>();
                    let data = json!({"topics":topics}).to_string();
                    return Some((
                        Ok::<_, Infallible>(Event::default().event("invalidate").data(data)),
                        (state, token, project, previous, false, Some(())),
                    ));
                }
                let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
                let mut previous = previous;
                loop {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    match Self::snapshot(&state, &token, project.clone()).await {
                        Ok(next) => {
                            // If a capability disappears, ask the client to clear the affected reads too.
                            let topics = previous
                                .keys()
                                .chain(next.keys())
                                .filter(|k| previous.get(*k) != next.get(*k))
                                .cloned()
                                .collect::<std::collections::BTreeSet<_>>();
                            previous = next;
                            if !topics.is_empty() {
                                return Some((
                                    Ok(Event::default()
                                        .event("invalidate")
                                        .data(json!({"topics":topics}).to_string())),
                                    (state, token, project, previous, false, Some(())),
                                ));
                            }
                            if tokio::time::Instant::now() >= deadline {
                                return Some((
                                    Ok(Event::default().event("reconnect").data("{}")),
                                    (state, token, project, previous, false, None),
                                ));
                            }
                        }
                        Err(error) => {
                            let event = if matches!(
                                error.0,
                                PlatformError::Unauthorized
                                    | PlatformError::Forbidden
                                    | PlatformError::NotFound
                            ) {
                                "access-lost"
                            } else {
                                "reconnect"
                            };
                            return Some((
                                Ok(Event::default().event(event).data("{}")),
                                (state, token, project, previous, false, None),
                            ));
                        }
                    }
                }
            },
        );
        let events = events.inspect(move |_| {
            let _ = &permit;
        });
        let mut response = Sse::new(events)
            .keep_alive(
                KeepAlive::new()
                    .interval(Duration::from_secs(15))
                    .text("keep-alive"),
            )
            .into_response();
        response.headers_mut().insert(
            "x-accel-buffering",
            axum::http::HeaderValue::from_static("no"),
        );
        response.headers_mut().insert(
            "cache-control",
            axum::http::HeaderValue::from_static("no-cache, no-transform"),
        );
        Ok(response)
    }
    async fn snapshot(
        state: &ApiState,
        token: &str,
        project: Option<String>,
    ) -> Result<std::collections::BTreeMap<String, i64>, ApiError> {
        let token = zeroize::Zeroizing::new(token.to_string());
        state
            .run(move |store| store.live_update_snapshot(&token, project.as_deref()))
            .await
    }
}
