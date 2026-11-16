use crate::{ApiState, api_error::ApiError};
use axum::{
    Extension, Json,
    extract::{Query, State},
};
use nexora_platform::{AuthContext, InstallationService, OpenAiConfiguration, PlatformError};
use nexora_platform_store::PlatformStore;
use serde_json::Value;
pub(crate) struct RatingFeedbackHandlers;
impl RatingFeedbackHandlers {
    pub async fn get(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let year = query
            .get("year")
            .and_then(|v| v.parse::<i32>().ok())
            .ok_or(ApiError(PlatformError::Validation("year required")))?;
        let target = query
            .get("member_id")
            .cloned()
            .unwrap_or_else(|| ctx.actor_id().to_owned());
        state
            .run(move |store| store.rating_feedback_for(&ctx, &target, year, false))
            .await
            .map(Json)
    }
    pub async fn generate(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Result<Json<Value>, ApiError> {
        let year = query
            .get("year")
            .and_then(|v| v.parse::<i32>().ok())
            .ok_or(ApiError(PlatformError::Validation("year required")))?;
        let target = query
            .get("member_id")
            .cloned()
            .unwrap_or_else(|| ctx.actor_id().to_owned());
        let (settings, config, namespace, permit) = state.reasoning_context()?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut store = PlatformStore::connect(&config, &namespace)?;
            let (key, model, effort, endpoint) = if let Some(config) =
                store.service_runtime_settings(InstallationService::Openai)?
            {
                let metadata: OpenAiConfiguration =
                    serde_json::from_value(config.settings).map_err(|_| PlatformError::Storage)?;
                let secret = config.secret.ok_or(PlatformError::Unavailable)?;
                (
                    zeroize::Zeroizing::new(
                        String::from_utf8(secret.to_vec()).map_err(|_| PlatformError::Storage)?,
                    ),
                    metadata.model,
                    metadata.effort,
                    metadata.endpoint,
                )
            } else {
                let settings = settings.ok_or(PlatformError::Unavailable)?;
                (
                    nexora_reasoning::SecretFile::read(&settings.key_file)
                        .map_err(|_| PlatformError::Unavailable)?,
                    settings.model,
                    settings.effort,
                    "https://api.openai.com/v1".into(),
                )
            };
            let client = nexora_reasoning::OpenAiClient::new(key, model)
                .and_then(|c| c.with_endpoint(&endpoint))
                .map_err(|_| PlatformError::Unavailable)?;
            let input = store.rating_feedback_for(&ctx, &target, year, true)?;
            let report = nexora_reasoning::DeveloperFeedback::run(&client, &input, &effort)
                .map_err(|_| PlatformError::Unavailable)?;
            store.finish_rating_feedback_for(&ctx, &target, year, &report)
        })
        .await
        .map_err(|_| ApiError(PlatformError::Unavailable))?
        .map(Json)
        .map_err(ApiError)
    }
}
