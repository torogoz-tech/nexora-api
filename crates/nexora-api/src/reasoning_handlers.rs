use crate::{ApiState, api_error::ApiError, reasoning_request::ReasoningRequest};
use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct ReasoningHandlers;
impl ReasoningHandlers {
    pub async fn evaluate(
        State(state): State<ApiState>,
        Extension(ctx): Extension<AuthContext>,
        Path((project, repository)): Path<(String, String)>,
        body: Result<Json<ReasoningRequest>, JsonRejection>,
    ) -> Result<Json<Value>, ApiError> {
        let Json(input) =
            body.map_err(|_| ApiError(PlatformError::Validation("invalid reasoning request")))?;
        Self::run(state, ctx, project, repository, input)
            .await
            .map(Json)
    }
    pub async fn run(
        state: ApiState,
        ctx: AuthContext,
        project: String,
        repository: String,
        mut input: ReasoningRequest,
    ) -> Result<Value, ApiError> {
        input
            .analysis
            .validate()
            .map_err(|_| ApiError(PlatformError::Validation("invalid analysis criteria")))?;
        let (settings, config, namespace, permit) = state.reasoning_context()?;
        let work = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut source = crate::reasoning_repository::ReasoningRepository {
                store: nexora_platform_store::PlatformStore::connect(&config, &namespace)?,
                ctx,
                project,
                repository,
                snapshot: input.snapshot,
                identity: None,
            };
            source.read("nexora_repository_overview", json!({"offset":0,"limit":1}))?;
            let mut baseline = if let Some(snapshot) = input.baseline_snapshot {
                let mut baseline = crate::reasoning_repository::ReasoningRepository {
                    store: nexora_platform_store::PlatformStore::connect(&config, &namespace)?,
                    ctx: source.ctx.clone(),
                    project: source.project.clone(),
                    repository: source.repository.clone(),
                    snapshot: Some(snapshot),
                    identity: None,
                };
                baseline.read("nexora_repository_overview", json!({"offset":0,"limit":1}))?;
                if baseline.identity == source.identity {
                    return Err(PlatformError::Validation(
                        "baseline must differ from current snapshot",
                    ));
                }
                Some(baseline)
            } else {
                None
            };
            let mut tools = nexora_reasoning::RegisteredRepository::tools();
            if baseline.is_some() {
                tools.push(nexora_reasoning::ComparisonTool::definition());
            }
            let mut requirement_provenance = Value::Null;
            if let Some(reference) = &input.requirement {
                if !matches!(
                    input.analysis.purpose.as_str(),
                    "requirement_connection" | "contribution_review"
                ) {
                    return Err(PlatformError::Validation(
                        "requirement reference requires a requirement or contribution review",
                    ));
                }
                let document = source.store.document_version(
                    &source.ctx,
                    &source.project,
                    &reference.document_id,
                    &reference.version_id,
                )?;
                requirement_provenance =
                    crate::requirement_reasoning_context::RequirementReasoningContext::prepare(
                        &mut input.analysis,
                        &document,
                    )?;
            }
            if input.evidence_ids.len() > 6 {
                return Err(PlatformError::Validation(
                    "at most6 evidence files per analysis",
                ));
            }
            let mut attachments = Vec::new();
            for id in &input.evidence_ids {
                let evidence = source
                    .store
                    .evidence_content(&source.ctx, &source.project, id)?;
                if evidence.record.repository_id != source.repository {
                    return Err(PlatformError::NotFound);
                }
                if let Some(snapshot) = &evidence.record.snapshot
                    && source.snapshot.as_ref() != Some(snapshot)
                    && baseline.as_ref().and_then(|s| s.snapshot.as_ref()) != Some(snapshot)
                {
                    return Err(PlatformError::Conflict(
                        "evidence belongs to a different snapshot",
                    ));
                }
                if let Some(document) = &evidence.record.document_id
                    && input.requirement.as_ref().is_none_or(|r| {
                        &r.document_id != document
                            || Some(&r.version_id) != evidence.record.document_version_id.as_ref()
                    })
                {
                    return Err(PlatformError::Conflict(
                        "evidence belongs to a different requirement version",
                    ));
                }
                attachments.push(nexora_reasoning::AnalysisAttachment {
                    id: evidence.record.id.clone(),
                    media_type: evidence.record.media_type.clone(),
                    data_base64: evidence.data_base64,
                    sha256: evidence.record.sha256.clone(),
                    metadata: serde_json::to_value(&evidence.record)
                        .map_err(|_| PlatformError::Storage)?,
                });
            }
            let (key, model, effort, endpoint) = if let Some(config) = source
                .store
                .service_runtime_settings(nexora_platform::InstallationService::Openai)?
            {
                let metadata: nexora_platform::OpenAiConfiguration =
                    serde_json::from_value(config.settings).map_err(|_| PlatformError::Storage)?;
                let bytes = config.secret.ok_or(PlatformError::Unavailable)?;
                let key = zeroize::Zeroizing::new(
                    String::from_utf8(bytes.to_vec()).map_err(|_| PlatformError::Storage)?,
                );
                (key, metadata.model, metadata.effort, metadata.endpoint)
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
            let session = nexora_reasoning::ReasoningSession::new(client)
                .with_effort(effort.clone())
                .map_err(|_| PlatformError::Unavailable)?;
            let mut access_error = None;
            let result = session.evaluate_with_attachments(
                &input.analysis,
                tools,
                &attachments,
                |tool, args, remaining| {
                    let started = std::time::Instant::now();
                    let read = if tool == nexora_reasoning::ComparisonTool::NAME {
                        if let Some(baseline) = &mut baseline {
                            source.compare_with(baseline, args, remaining)
                        } else {
                            Err(PlatformError::Forbidden)
                        }
                    } else {
                        source.read(tool, args)
                    };
                    if started.elapsed() >= remaining {
                        return Err(nexora_reasoning::ReasoningError::Budget);
                    }
                    match read {
                        Ok(value) => Ok(value),
                        Err(error) => {
                            access_error = Some(error);
                            Err(nexora_reasoning::ReasoningError::Repository)
                        }
                    }
                },
            );
            if let Some(error) = access_error {
                return Err(error);
            }
            let (report, evidence) = result.map_err(|_| PlatformError::Unavailable)?;
            source.read("nexora_repository_overview", json!({"offset":0,"limit":1}))?;
            if let Some(baseline) = &mut baseline {
                baseline.read("nexora_repository_overview", json!({"offset":0,"limit":1}))?;
            }
            for attachment in &attachments {
                let current =
                    source
                        .store
                        .evidence_content(&source.ctx, &source.project, &attachment.id)?;
                if current.record.sha256 != attachment.sha256 {
                    return Err(PlatformError::Conflict(
                        "evidence changed during evaluation",
                    ));
                }
            }
            if let Some(reference) = &input.requirement {
                let current = source.store.document_version(
                    &source.ctx,
                    &source.project,
                    &reference.document_id,
                    &reference.version_id,
                )?;
                crate::requirement_reasoning_context::RequirementReasoningContext::revalidate(
                    &requirement_provenance,
                    &current,
                )?;
            }
            Ok(
                json!({"provider":"openai","model":model,"reasoning_effort":effort,"identity":source.identity,"baseline":baseline.as_ref().and_then(|s|s.identity.as_ref()),"requirement":requirement_provenance,"request":input.analysis,"report":report,"evidence":evidence,"authority":"inferred_not_approved"}),
            )
        });
        // Blocking database operations retain their platform timeouts. A timed-out
        // task keeps its dedicated permit until it drains; it cannot publish a late result.
        tokio::time::timeout(std::time::Duration::from_secs(180), work)
            .await
            .map_err(|_| ApiError(PlatformError::Unavailable))?
            .map_err(|_| ApiError(PlatformError::Unavailable))?
            .map_err(ApiError)
    }
}
