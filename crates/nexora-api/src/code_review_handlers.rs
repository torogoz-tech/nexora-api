use crate::{
    ApiState, api_error::ApiError, code_review_request::CodeReviewRequest, mutation_id::MutationId,
};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::HeaderMap,
};
use nexora_platform::{AuthContext, InstallationService, OpenAiConfiguration, PlatformError};
use nexora_platform_store::{CodeReviewStore, PlatformStore};
use serde_json::Value;

pub(crate) struct CodeReviewHandlers;
impl CodeReviewHandlers {
    pub async fn list(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, number)): Path<(String, String, u64)>,
    ) -> Result<Json<Value>, ApiError> {
        let policy = state.git_policy();
        state
            .run(move |store| {
                CodeReviewStore::new(store).list(&ctx, &project, &repository, number, &policy)
            })
            .await
            .map(Json)
    }
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, number, id)): Path<(String, String, u64, String)>,
    ) -> Result<Json<Value>, ApiError> {
        let policy = state.git_policy();
        state
            .run(move |store| {
                CodeReviewStore::new(store).get(&ctx, &project, &repository, number, &id, &policy)
            })
            .await
            .map(Json)
    }
    pub async fn request(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository, number)): Path<(String, String, u64)>,
        headers: HeaderMap,
        body: Result<Json<CodeReviewRequest>, JsonRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid PR review request")))?;
        let request_id = MutationId::required(&headers)?;
        let policy = state.git_policy();
        let (claim_ctx, claim_project, claim_repository, claim_policy) = (
            ctx.clone(),
            project.clone(),
            repository.clone(),
            policy.clone(),
        );
        let prepared = state
            .run(move |store| {
                CodeReviewStore::new(store).prepare(
                    &claim_ctx,
                    &claim_project,
                    &claim_repository,
                    number,
                    &request_id,
                    &input.expected_head_sha,
                    &claim_policy,
                )
            })
            .await?;
        let Some(input) = prepared.input else {
            return Ok(Json(prepared.review));
        };
        let id = prepared.review["id"]
            .as_str()
            .ok_or(ApiError(PlatformError::Storage))?
            .to_owned();
        // An exact retry already returned its stored state above. Only the durable
        // claim winner may acquire model capacity and incur an external call.
        let (settings, config, namespace, permit) = match state.reasoning_context() {
            Ok(value) => value,
            Err(_) => {
                return state
                    .run(move |store| {
                        CodeReviewStore::new(store).finish(
                            &ctx,
                            &project,
                            &repository,
                            number,
                            &id,
                            None,
                            "",
                            &policy,
                        )
                    })
                    .await
                    .map(Json);
            }
        };
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut store = PlatformStore::connect(&config, &namespace)?;
            let model_result =
                (|| -> Result<(nexora_reasoning::OpenAiClient, String, String), PlatformError> {
                    let (key, model, effort, endpoint) = if let Some(config) =
                        store.service_runtime_settings(InstallationService::Openai)?
                    {
                        let metadata: OpenAiConfiguration = serde_json::from_value(config.settings)
                            .map_err(|_| PlatformError::Storage)?;
                        let secret = config.secret.ok_or(PlatformError::Unavailable)?;
                        (
                            zeroize::Zeroizing::new(
                                String::from_utf8(secret.to_vec())
                                    .map_err(|_| PlatformError::Storage)?,
                            ),
                            metadata.model,
                            metadata.effort,
                            metadata.endpoint,
                        )
                    } else {
                        let settings = settings.ok_or(PlatformError::Unavailable)?;
                        let key = nexora_reasoning::SecretFile::read(&settings.key_file)
                            .map_err(|_| PlatformError::Unavailable)?;
                        (
                            key,
                            settings.model,
                            settings.effort,
                            "https://api.openai.com/v1".into(),
                        )
                    };
                    let client = nexora_reasoning::OpenAiClient::new(key, model.clone())
                        .and_then(|client| client.with_endpoint(&endpoint))
                        .map_err(|_| PlatformError::Unavailable)?;
                    Ok((client, model, effort))
                })();
            let (report, model) = match model_result {
                Ok((client, model, effort)) => (
                    nexora_reasoning::PullRequestReview::run(&client, &input, &effort).ok(),
                    model,
                ),
                Err(_) => (None, String::new()),
            };
            CodeReviewStore::new(&mut store).finish(
                &ctx,
                &project,
                &repository,
                number,
                &id,
                report,
                &model,
                &policy,
            )
        })
        .await
        .map_err(|_| ApiError(PlatformError::Unavailable))?
        .map(Json)
        .map_err(ApiError)
    }
}
