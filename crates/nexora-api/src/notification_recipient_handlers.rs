use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::extract::Path;
use axum::{
    Extension, Json,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct NotificationRecipientHandlers;
impl NotificationRecipientHandlers {
    pub async fn recipients(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
    ) -> Result<Json<Vec<NotificationRecipient>>, ApiError> {
        state
            .run(move |s| s.notification_recipients(&ctx))
            .await
            .map(Json)
    }
    pub async fn change_recipient(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path(actor): Path<String>,
        headers: HeaderMap,
        body: Result<Json<NotificationRecipientChange>, JsonRejection>,
    ) -> Result<MutationResponse<NotificationRecipient>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid recipient")))?;
        let id = MutationId::required(&headers)?;
        let result = state
            .mutate(
                ctx,
                "notification.recipient.change",
                id.clone(),
                json!({"actor":actor,"input":input}),
                move |s, c| s.change_notification_recipient(c, &actor, &input),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::OK,
            request_id: id,
            body: Some(result),
        })
    }
}
