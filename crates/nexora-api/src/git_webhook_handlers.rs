use crate::{
    ApiState, api_error::ApiError, mutation_id::MutationId, mutation_response::MutationResponse,
};
use axum::{
    Extension, Json,
    body::Bytes,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use nexora_platform::*;
use nexora_platform_store::WebhookProof;
use serde_json::{Value, json};
pub(crate) struct GitWebhookHandlers;
impl GitWebhookHandlers {
    fn header(headers: &HeaderMap, name: &str) -> Result<String, ApiError> {
        let values: Vec<_> = headers.get_all(name).iter().collect();
        if values.len() != 1 {
            return Err(ApiError(PlatformError::Unauthorized));
        }
        values[0]
            .to_str()
            .map(str::to_owned)
            .map_err(|_| ApiError(PlatformError::Unauthorized))
    }
    pub async fn github(
        State(state): State<ApiState>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Result<(StatusCode, Json<WebhookReceipt>), ApiError> {
        let proof = WebhookProof {
            delivery_id: Self::header(&headers, "X-GitHub-Delivery")?,
            event: Self::header(&headers, "X-GitHub-Event")?,
            signature: Self::header(&headers, "X-Hub-Signature-256")?,
            timestamp: None,
        };
        let policy = state.git_policy();
        let response = state
            .run(move |s| s.receive_github_webhook(&proof, &body, &policy))
            .await?;
        Ok((StatusCode::ACCEPTED, Json(response)))
    }
    pub async fn gitlab(
        State(state): State<ApiState>,
        Path(id): Path<String>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Result<(StatusCode, Json<WebhookReceipt>), ApiError> {
        let proof = WebhookProof {
            delivery_id: Self::header(&headers, "webhook-id")?,
            event: Self::header(&headers, "X-Gitlab-Event")?,
            signature: Self::header(&headers, "webhook-signature")?,
            timestamp: Some(Self::header(&headers, "webhook-timestamp")?),
        };
        let policy = state.git_policy();
        let response = state
            .run(move |s| s.receive_gitlab_webhook(&id, &proof, &body, &policy))
            .await?;
        Ok((StatusCode::ACCEPTED, Json(response)))
    }
    pub async fn register(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        headers: HeaderMap,
        body: Result<Json<GitlabWebhookRegistration>, JsonRejection>,
    ) -> Result<MutationResponse<GitlabWebhook>, ApiError> {
        let Json(input) = body.map_err(|_| {
            ApiError(PlatformError::Validation(
                "invalid GitLab webhook registration",
            ))
        })?;
        let request_id = MutationId::read(&headers)?;
        let policy = state.git_policy();
        let data = json!({"project_id":project,"repository_id":repository,"registration":input});
        let body = state
            .mutate(
                ctx,
                "gitlab.webhook.register",
                request_id.clone(),
                data,
                move |s, c| s.register_gitlab_webhook(c, &project, &repository, &input, &policy),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::CREATED,
            request_id,
            body: Some(body),
        })
    }
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
    ) -> Result<Json<Value>, ApiError> {
        let items = state
            .run(move |s| s.gitlab_webhooks(&ctx, &project, &repository))
            .await?;
        Ok(Json(json!({"items":items})))
    }
    pub async fn revoke(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, id)): Path<(String, String, String)>,
        headers: HeaderMap,
    ) -> Result<MutationResponse<()>, ApiError> {
        let request_id = MutationId::read(&headers)?;
        let data = json!({"project_id":project,"repository_id":repository,"webhook_id":id});
        state
            .mutate(
                ctx,
                "gitlab.webhook.revoke",
                request_id.clone(),
                data,
                move |s, c| s.revoke_gitlab_webhook(c, &project, &repository, &id),
            )
            .await?;
        Ok(MutationResponse {
            status: StatusCode::NO_CONTENT,
            request_id,
            body: None,
        })
    }
}
