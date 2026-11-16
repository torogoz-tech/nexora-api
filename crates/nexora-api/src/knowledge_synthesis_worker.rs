use crate::ApiState;
use nexora_platform::{InstallationService, OpenAiConfiguration, PlatformError};
use nexora_platform_store::PlatformStore;
pub(crate) struct KnowledgeSynthesisWorker;
impl KnowledgeSynthesisWorker {
    pub async fn tick(state: &ApiState) {
        let Ok((settings, config, namespace, permit)) = state.reasoning_context() else {
            return;
        };
        let result = tokio::task::spawn_blocking(move || -> Result<(), PlatformError> {
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
                let Some(settings) = settings else {
                    return Ok(());
                };
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
            if let Some(job) = store.claim_knowledge_synthesis()? {
                let id = job["id"].as_str().ok_or(PlatformError::Storage)?;
                let result = nexora_reasoning::DocumentSynthesis::run(&client, &job, &effort);
                store.finish_knowledge_synthesis(
                    id,
                    job["source_revision"]
                        .as_i64()
                        .ok_or(PlatformError::Storage)?,
                    result.as_deref().ok(),
                )?;
            }
            if let Some(rag_job) = store.claim_knowledge_processing()? {
                let source = store.knowledge_processing_source(&rag_job)?;
                let rag_result = (|| -> Result<(), PlatformError> {
                    let structure = nexora_reasoning::KnowledgeStructuring::run(
                        &client,
                        rag_job["source_kind"].as_str().unwrap_or("knowledge"),
                        &source.0,
                        &source.1,
                        &effort,
                    )
                    .map_err(|_| PlatformError::Unavailable)?;
                    store.finish_knowledge_processing(
                        &rag_job, &source.0, &source.1, source.2, &structure,
                    )?;
                    let embedding_model = store
                        .service_runtime_settings(InstallationService::Openai)?
                        .and_then(|settings| {
                            serde_json::from_value::<OpenAiConfiguration>(settings.settings).ok()
                        })
                        .map(|settings| settings.embedding_model)
                        .unwrap_or_else(|| "text-embedding-3-small".into());
                    store.publish_knowledge_embeddings(&rag_job, &client, &embedding_model)
                })();
                if let Err(error) = rag_result {
                    store.fail_knowledge_processing(&rag_job, &error.to_string())?;
                }
            }
            Ok(())
        })
        .await;
        if !matches!(result, Ok(Ok(()))) {
            eprintln!("document synthesis unavailable");
        }
    }
}
