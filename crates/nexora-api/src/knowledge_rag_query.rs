use crate::{ApiState, api_error::ApiError};
use nexora_platform::{AuthContext, InstallationService, OpenAiConfiguration, PlatformError};
use nexora_platform_store::PlatformStore;
use serde_json::Value;

pub(crate) struct KnowledgeRagQuery;

impl KnowledgeRagQuery {
    pub async fn search(
        state: ApiState,
        ctx: AuthContext,
        project: Option<String>,
        search: String,
    ) -> Result<Value, ApiError> {
        let authorization_project = project.clone();
        let authorization_ctx = ctx.clone();
        state
            .run(move |store| {
                store.authorize_knowledge_context_search(
                    &authorization_ctx,
                    authorization_project.as_deref(),
                )
            })
            .await?;
        let embedding = Self::query_embedding(&state, &search).await;
        let embedding_model = embedding.as_ref().map(|(_, model)| model.clone());
        let embedding_values = embedding.as_ref().map(|(values, _)| values.clone());
        state
            .run(move |store| {
                store.knowledge_context_search_with_embedding(
                    &ctx,
                    project.as_deref(),
                    &search,
                    embedding_values.as_deref(),
                    embedding_model.as_deref(),
                )
            })
            .await
    }

    async fn query_embedding(state: &ApiState, search: &str) -> Option<(Vec<f32>, String)> {
        let search = search.to_owned();
        let Ok((settings, config, namespace, permit)) = state.reasoning_context() else {
            return None;
        };
        tokio::task::spawn_blocking(move || -> Result<(Vec<f32>, String), PlatformError> {
            let _permit = permit;
            let mut store = PlatformStore::connect(&config, &namespace)?;
            let (key, model, endpoint, embedding_model) = if let Some(service) =
                store.service_runtime_settings(InstallationService::Openai)?
            {
                let configuration: OpenAiConfiguration =
                    serde_json::from_value(service.settings).map_err(|_| PlatformError::Storage)?;
                let secret = service.secret.ok_or(PlatformError::Unavailable)?;
                (
                    zeroize::Zeroizing::new(
                        String::from_utf8(secret.to_vec()).map_err(|_| PlatformError::Storage)?,
                    ),
                    configuration.model,
                    configuration.endpoint,
                    configuration.embedding_model,
                )
            } else {
                let settings = settings.ok_or(PlatformError::Unavailable)?;
                (
                    nexora_reasoning::SecretFile::read(&settings.key_file)
                        .map_err(|_| PlatformError::Unavailable)?,
                    settings.model,
                    "https://api.openai.com/v1".to_owned(),
                    "text-embedding-3-small".to_owned(),
                )
            };
            let client = nexora_reasoning::OpenAiClient::new(key, model)
                .and_then(|client| client.with_endpoint(&endpoint))
                .map_err(|_| PlatformError::Unavailable)?;
            client
                .embed(&embedding_model, &search)
                .map(|embedding| (embedding, embedding_model))
                .map_err(|_| PlatformError::Unavailable)
        })
        .await
        .ok()
        .and_then(Result::ok)
    }
}
