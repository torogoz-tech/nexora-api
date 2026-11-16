use crate::{ApiState, api_error::ApiError};
use axum::{
    extract::State,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
};
use futures_util::{StreamExt, stream};
use std::{convert::Infallible, time::Duration};
pub(crate) struct InstallationPreferencesEvents;
impl InstallationPreferencesEvents {
    pub async fn subscribe(State(state): State<ApiState>) -> Result<Response, ApiError> {
        let permit = state.public_live_update_permit()?;
        let initial = state
            .run(|store| store.public_installation_preferences())
            .await?;
        let events = stream::unfold(
            (state, initial, true, true),
            |(state, previous, first, alive)| async move {
                if !alive {
                    return None;
                }
                if first {
                    let data = serde_json::to_string(&previous).unwrap_or_default();
                    return Some((
                        Ok::<_, Infallible>(Event::default().event("preferences").data(data)),
                        (state, previous, false, true),
                    ));
                }
                let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
                loop {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    match state
                        .run(|store| store.public_installation_preferences())
                        .await
                    {
                        Ok(next) if next.revision != previous.revision => {
                            let data = serde_json::to_string(&next).unwrap_or_default();
                            return Some((
                                Ok(Event::default().event("preferences").data(data)),
                                (state, next, false, true),
                            ));
                        }
                        Ok(_) if tokio::time::Instant::now() < deadline => {}
                        _ => {
                            return Some((
                                Ok(Event::default().event("reconnect").data("{}")),
                                (state, previous, false, false),
                            ));
                        }
                    }
                }
            },
        )
        .inspect(move |_| {
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
        Ok(response)
    }
}
