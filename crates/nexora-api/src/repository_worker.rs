use crate::{
    RepositoryWorkerOutcome, RepositoryWorkerSettings,
    repository_worker_directory::RepositoryWorkerDirectory,
    repository_worker_heartbeat::RepositoryWorkerHeartbeat,
};
use nexora_core::{NexoraError, RepositoryId, TenantId};
use nexora_indexer::Indexer;
use nexora_language::LanguageRegistry;
use nexora_platform::{
    InstallationService, OpenAiConfiguration, PlatformError, RepositoryBinding,
    RepositoryJobFailure,
};
use nexora_platform_store::{GitNetworkPolicy, PlatformStore, RepositoryJobLease};
use nexora_runtime::{DatabaseConnections, DatabaseRuntime};
use nexora_source::{
    GitFetchPolicy, GitFetcher, GitMaterializer, GitObjectReader, GitTransferLimits, SourceLimits,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

pub struct RepositoryWorker {
    connections: DatabaseConnections,
    namespace: String,
    settings: RepositoryWorkerSettings,
    policy: GitNetworkPolicy,
    store: PlatformStore,
    runtime: DatabaseRuntime,
}
impl RepositoryWorker {
    pub fn connect(
        connections: &DatabaseConnections,
        namespace: &str,
        settings: RepositoryWorkerSettings,
        policy: GitNetworkPolicy,
    ) -> Result<Self, PlatformError> {
        settings.validate()?;
        policy.validate()?;
        Ok(Self {
            connections: connections.clone(),
            namespace: namespace.into(),
            settings,
            policy,
            store: PlatformStore::connect(connections, namespace)?,
            runtime: DatabaseRuntime::connect(connections).map_err(|_| PlatformError::Storage)?,
        })
    }
    pub fn run_once(&mut self) -> Result<Option<RepositoryWorkerOutcome>, PlatformError> {
        self.store.resume_one_task_transition(&self.policy)?;
        self.store.compose_one_release(
            &self.policy,
            &self.settings.git_binary,
            &self.settings.source_root,
        )?;
        let Some(lease) = self.store.claim_repository_job(&self.settings.id, 300)? else {
            return Ok(None);
        };
        let monitor = PlatformStore::connect(&self.connections, &self.namespace)?;
        let lost = AtomicBool::new(false);
        let result = std::thread::scope(|threads| {
            // Own the sender inside the scope: unwinding also disconnects the
            // monitor, so a parser panic cannot leave scope waiting forever.
            let (stop, listen) = mpsc::channel();
            let lease_ref = &lease;
            let lost_ref = &lost;
            let heartbeat = threads.spawn(move || {
                RepositoryWorkerHeartbeat::run(monitor, lease_ref, listen, lost_ref);
            });
            let result = self.execute(&lease, &lost);
            drop(stop);
            // The monitor touches only lease state. Publication always rechecks SQL
            // authority itself, including cancellation and concurrent revocation.
            let _ = heartbeat.join();
            result
        });
        match result {
            Ok(binding) => Ok(Some(RepositoryWorkerOutcome::published(binding))),
            Err(failure) => self
                .store
                .fail_repository_job(&lease, failure)
                .map(|job| Some(job.into())),
        }
    }
    fn checkpoint(
        &mut self,
        lease: &RepositoryJobLease,
        lost: &AtomicBool,
    ) -> Result<(), RepositoryJobFailure> {
        if lost.load(Ordering::Acquire) {
            return Err(RepositoryJobFailure::SourceUnavailable);
        }
        self.store
            .renew_repository_job(lease, 300)
            .map(|_| ())
            .map_err(Self::source_failure)
    }
    fn source_failure(error: PlatformError) -> RepositoryJobFailure {
        match error {
            PlatformError::Unauthorized | PlatformError::Forbidden | PlatformError::NotFound => {
                RepositoryJobFailure::SourceAccessDenied
            }
            PlatformError::Unavailable => RepositoryJobFailure::SourceUnavailable,
            PlatformError::TooManyRequests => RepositoryJobFailure::SourceAccessDenied,
            PlatformError::Storage => RepositoryJobFailure::StorageUnavailable,
            PlatformError::Conflict(_) | PlatformError::Validation(_) => {
                RepositoryJobFailure::SourceInvalid
            }
        }
    }
    fn analysis_failure(error: NexoraError) -> RepositoryJobFailure {
        match &error {
            NexoraError::Storage(message) | NexoraError::Limit(message) => {
                eprintln!(
                    "repository staging failed: {}",
                    message.chars().take(256).collect::<String>()
                );
            }
            _ => eprintln!("repository staging failed: analysis rejected; see file diagnostics"),
        }
        match error {
            NexoraError::Storage(_) => RepositoryJobFailure::StorageUnavailable,
            _ => RepositoryJobFailure::AnalysisFailed,
        }
    }
    fn execute(
        &mut self,
        lease: &RepositoryJobLease,
        lost: &AtomicBool,
    ) -> Result<RepositoryBinding, RepositoryJobFailure> {
        let multiple = self.store.repository_job_analysis_profiles(lease)
            .map_err(Self::source_failure)?
            .map(crate::repository_worker_analysis_profiles::RepositoryWorkerAnalysisProfiles::compile)
            .transpose()?;
        // Jobs created before the multi-profile migration retain their exact
        // scalar pin or operator fallback. Never reinterpret those jobs.
        let legacy = if multiple.is_none() {
            let managed = self
                .store
                .repository_job_analysis_profile(lease)
                .map_err(Self::source_failure)?;
            let indexer = if let Some(package) = managed {
                nexora_indexer::AnalysisProfileCompiler::compile(&package)
                    .map_err(|_| RepositoryJobFailure::ProfileUnavailable)?
            } else {
                let profile = self
                    .settings
                    .profiles
                    .get(&lease.job.repository_id)
                    .ok_or(RepositoryJobFailure::ProfileUnavailable)?;
                let registry = LanguageRegistry::load(&profile.directory)
                    .map_err(|_| RepositoryJobFailure::ProfileUnavailable)?;
                let language = registry
                    .get(&profile.language_id)
                    .cloned()
                    .ok_or(RepositoryJobFailure::ProfileUnavailable)?;
                if language.manifest.composition.is_some() {
                    let dependencies = registry
                        .composition_dependencies(&language)
                        .map_err(|_| RepositoryJobFailure::ProfileUnavailable)?;
                    Indexer::with_catalog(language, dependencies)
                        .map_err(|_| RepositoryJobFailure::ProfileUnavailable)?
                } else {
                    Indexer::new(language)
                }
            };
            Some(indexer)
        } else {
            None
        };
        let plan = self
            .store
            .prepare_repository_job_source(lease, &self.policy)
            .map_err(Self::source_failure)?;
        self.store
            .import_repository_commits(lease, &self.policy)
            .map_err(Self::source_failure)?;
        self.store
            .import_repository_branches(lease, &self.policy)
            .map_err(Self::source_failure)?;
        let workspace = RepositoryWorkerDirectory::create(&self.settings.source_root)
            .map_err(|_| RepositoryJobFailure::SourceInvalid)?;
        let fetcher = GitFetcher::new(
            self.settings.git_binary.clone(),
            GitTransferLimits::default(),
        )
        .map_err(|_| RepositoryJobFailure::SourceInvalid)?;
        let policy =
            GitFetchPolicy::new(plan.origin()).map_err(|_| RepositoryJobFailure::SourceInvalid)?;
        self.checkpoint(lease, lost)?;
        let fetched = fetcher
            .fetch(
                &policy,
                plan.repository(),
                plan.commit(),
                plan.format(),
                plan.credentials(),
                &workspace.path().join("download"),
            )
            .map_err(|error| {
                eprintln!("repository fetch failed: {error}");
                RepositoryJobFailure::SourceUnavailable
            })?;
        self.checkpoint(lease, lost)?;
        let reader =
            GitObjectReader::new(self.settings.git_binary.clone(), Duration::from_secs(60))
                .map_err(|_| RepositoryJobFailure::SourceInvalid)?;
        let captured = workspace.path().join("source");
        GitMaterializer::materialize(
            &reader,
            fetched.bare(),
            plan.commit(),
            plan.format(),
            &captured,
            &SourceLimits::default(),
        )
        .map_err(|error| {
            eprintln!("repository extraction failed: {error}");
            RepositoryJobFailure::SourceInvalid
        })?;
        self.checkpoint(lease, lost)?;
        let tenant = TenantId(plan.scope().tenant_id.clone());
        let repository = RepositoryId(plan.scope().repository_id.clone());
        let (result, selection) = if let Some(indexer) = multiple.as_ref() {
            let (result, selection) = self
                .runtime
                .stage_repository_profiles(indexer, &captured, tenant.clone(), repository.clone())
                .map_err(Self::analysis_failure)?;
            (result, Some(selection))
        } else {
            let indexer = legacy
                .as_ref()
                .ok_or(RepositoryJobFailure::ProfileUnavailable)?;
            let result = self
                .runtime
                .stage_repository(indexer, &captured, tenant.clone(), repository.clone())
                .map_err(Self::analysis_failure)?;
            (result, None)
        };
        self.checkpoint(lease, lost)?;
        let header = self
            .runtime
            .query_header(&tenant, &repository, Some(&result.snapshot.id))
            .map_err(|_| RepositoryJobFailure::StorageUnavailable)?;
        if let Some(service) = self
            .store
            .service_runtime_settings(InstallationService::Openai)
            .map_err(Self::source_failure)?
        {
            let configuration: OpenAiConfiguration = serde_json::from_value(service.settings)
                .map_err(|_| RepositoryJobFailure::SourceInvalid)?;
            let secret = service
                .secret
                .ok_or(RepositoryJobFailure::SourceUnavailable)?;
            let key = zeroize::Zeroizing::new(
                String::from_utf8(secret.to_vec())
                    .map_err(|_| RepositoryJobFailure::SourceUnavailable)?,
            );
            let client = nexora_reasoning::OpenAiClient::new(key, configuration.model)
                .and_then(|client| client.with_endpoint(&configuration.endpoint))
                .map_err(|_| RepositoryJobFailure::SourceInvalid)?;
            self.store
                .index_code_snapshot_embeddings(
                    &lease.job.project_id,
                    &lease.job.repository_id,
                    &result.snapshot.id.0,
                    &result.snapshot.sources,
                    &client,
                    &configuration.embedding_model,
                )
                .map_err(Self::source_failure)?;
        }
        match selection {
            Some(selection) => self
                .store
                .complete_repository_job_with_profile_selection(lease, &header, &selection),
            None => self.store.complete_repository_job(lease, &header),
        }
        .map_err(Self::source_failure)
    }
}

#[cfg(test)]
#[path = "repository_worker_failure_tests.rs"]
mod failure_tests;
