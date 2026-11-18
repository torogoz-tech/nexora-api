use axum::{
    Json,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use nexora_platform::WorkTask;
use serde_json::{Value, json};

pub(crate) struct TaskTransitionMutationResponse {
    pub(crate) task: Option<WorkTask>,
    pub(crate) task_id: String,
    pub(crate) request_id: String,
    pub(crate) waiting_for_knowledge: bool,
}

impl IntoResponse for TaskTransitionMutationResponse {
    fn into_response(self) -> Response {
        let status = if self.waiting_for_knowledge {
            StatusCode::ACCEPTED
        } else {
            StatusCode::OK
        };
        let body = self.task.map_or_else(
            || {
                Json(json!({
                    "accepted": true,
                    "task_id": self.task_id,
                    "status": "waiting_knowledge"
                }))
            },
            |task| Json(serde_json::to_value(task).unwrap_or(Value::Null)),
        );
        let mut response = (status, body).into_response();
        if let Ok(value) = self.request_id.parse() {
            response.headers_mut().insert("idempotency-key", value);
        }
        if self.waiting_for_knowledge {
            response.headers_mut().insert(
                "x-nexora-transition-status",
                HeaderValue::from_static("waiting_knowledge"),
            );
        }
        response
    }
}

impl TaskTransitionMutationResponse {
    pub(crate) fn mcp_content(&self) -> Value {
        match &self.task {
            Some(task) => serde_json::to_value(task).unwrap_or(Value::Null),
            None => json!({
                "accepted": true,
                "task_id": self.task_id,
                "status": "waiting_knowledge"
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TaskTransitionMutationResponse;
    use axum::{body::to_bytes, response::IntoResponse};
    use serde_json::{Value, json};

    #[tokio::test]
    async fn pending_transition_is_accepted_with_a_compatible_status_envelope() {
        let response = TaskTransitionMutationResponse {
            task: None,
            task_id: "task_123".into(),
            request_id: "transition_123".into(),
            waiting_for_knowledge: true,
        }
        .into_response();

        assert_eq!(response.status(), axum::http::StatusCode::ACCEPTED);
        assert_eq!(
            response.headers()["x-nexora-transition-status"],
            "waiting_knowledge"
        );
        assert_eq!(response.headers()["idempotency-key"], "transition_123");
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value,
            json!({"accepted":true,"task_id":"task_123","status":"waiting_knowledge"})
        );
    }
}
