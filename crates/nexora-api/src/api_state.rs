use crate::api_error::ApiError;
use crate::store_pool::StorePool;
use crate::task_workflow_pool::TaskWorkflowPool;
use nexora_platform::PlatformError;
use nexora_platform_store::PlatformStore;
use nexora_runtime::DatabaseConnections;
use nexora_task_workflow::{GitNetworkPolicy as TaskWorkflowGitNetworkPolicy, TaskWorkflowStore};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::Semaphore;
/// Reuses connections only; no business or authorization results are cached in RAM.
#[derive(Clone)]
pub struct ApiState {
    public_live_permits: Arc<Semaphore>,
    live_permits: Arc<Semaphore>,
    reasoning: Option<crate::ReasoningSettings>,
    reasoning_permits: Arc<Semaphore>,
    git_policy: Arc<nexora_platform_store::GitNetworkPolicy>,
    task_workflow_git_policy: Arc<TaskWorkflowGitNetworkPolicy>,
    identity_policy: Arc<nexora_platform_store::OidcNetworkPolicy>,
    trusted_proxies: Arc<Vec<IpAddr>>,
    config: Arc<DatabaseConnections>,
    namespace: Arc<String>,
    pool: Arc<StorePool>,
    task_workflow_pool: Arc<TaskWorkflowPool>,
    permits: Arc<Semaphore>,
    job_wakeup_sender: Option<tokio::sync::mpsc::Sender<()>>,
}
impl ApiState {
    pub async fn new(
        config: DatabaseConnections,
        namespace: String,
        workers: usize,
    ) -> Result<Self, PlatformError> {
        Self::with_identity_policy(
            config,
            namespace,
            workers,
            nexora_platform_store::OidcNetworkPolicy::default(),
        )
        .await
    }
    pub async fn with_identity_policy(
        config: DatabaseConnections,
        namespace: String,
        workers: usize,
        policy: nexora_platform_store::OidcNetworkPolicy,
    ) -> Result<Self, PlatformError> {
        Self::with_policies(
            config,
            namespace,
            workers,
            policy,
            nexora_platform_store::GitNetworkPolicy::default(),
        )
        .await
    }
    pub async fn with_policies(
        config: DatabaseConnections,
        namespace: String,
        workers: usize,
        policy: nexora_platform_store::OidcNetworkPolicy,
        git_policy: nexora_platform_store::GitNetworkPolicy,
    ) -> Result<Self, PlatformError> {
        git_policy.validate()?;
        policy.validate()?;
        if workers == 0 || workers > 32 {
            return Err(PlatformError::Validation("worker count must be 1..32"));
        }
        let connect_config = config.clone();
        let connect_namespace = namespace.clone();
        let workflow_config = config.clone();
        let workflow_namespace = namespace.clone();
        let (store, task_workflow_store) = tokio::task::spawn_blocking(move || {
            Ok::<_, PlatformError>((
                PlatformStore::connect(&connect_config, &connect_namespace)?,
                TaskWorkflowStore::connect(&workflow_config, &workflow_namespace)?,
            ))
        })
        .await
        .map_err(|_| PlatformError::Unavailable)??;
        let task_workflow_git_policy = TaskWorkflowGitNetworkPolicy {
            gitlab_origin: git_policy.gitlab_origin.clone(),
            callback_origin: git_policy.callback_origin.clone(),
            github_origin: git_policy.github_origin.clone(),
            github_api_origin: git_policy.github_api_origin.clone(),
        };
        Ok(Self {
            public_live_permits: Arc::new(Semaphore::new(64)),
            live_permits: Arc::new(Semaphore::new(64)),
            reasoning: None,
            reasoning_permits: Arc::new(Semaphore::new(1)),
            git_policy: Arc::new(git_policy),
            task_workflow_git_policy: Arc::new(task_workflow_git_policy),
            identity_policy: Arc::new(policy),
            trusted_proxies: Arc::new(Vec::new()),
            config: Arc::new(config),
            namespace: Arc::new(namespace),
            pool: Arc::new(StorePool::new(store)),
            task_workflow_pool: Arc::new(TaskWorkflowPool::new(task_workflow_store)),
            permits: Arc::new(Semaphore::new(workers)),
            job_wakeup_sender: None,
        })
    }
    pub fn with_job_wakeup_sender(mut self, sender: tokio::sync::mpsc::Sender<()>) -> Self {
        self.job_wakeup_sender = Some(sender);
        self
    }
    pub(crate) fn signal_job_wakeup(&self) {
        if let Some(sender) = &self.job_wakeup_sender {
            let _ = sender.try_send(());
        }
    }
    pub fn with_reasoning(mut self, settings: Option<crate::ReasoningSettings>) -> Self {
        self.reasoning = settings;
        self
    }
    pub fn with_trusted_proxies(mut self, proxies: Vec<IpAddr>) -> Self {
        self.trusted_proxies = Arc::new(proxies);
        self
    }
    pub(crate) fn client_address(
        &self,
        peer: Option<SocketAddr>,
        forwarded_for: Option<&str>,
    ) -> Option<String> {
        let peer = peer?.ip();
        if !self.trusted_proxies.contains(&peer) {
            return Some(peer.to_string());
        }
        let Some(forwarded_for) = forwarded_for else {
            return Some(peer.to_string());
        };
        let forwarded = forwarded_for
            .split(',')
            .map(str::trim)
            .map(str::parse::<IpAddr>)
            .collect::<Result<Vec<_>, _>>()
            .ok()
            .filter(|addresses| !addresses.is_empty() && addresses.len() <= 16);
        let Some(forwarded) = forwarded else {
            return Some(peer.to_string());
        };
        forwarded
            .iter()
            .rev()
            .chain(std::iter::once(&peer))
            .find(|address| !self.trusted_proxies.contains(address))
            .map(ToString::to_string)
    }
    pub(crate) fn reasoning_context(
        &self,
    ) -> Result<
        (
            Option<crate::ReasoningSettings>,
            DatabaseConnections,
            String,
            tokio::sync::OwnedSemaphorePermit,
        ),
        ApiError,
    > {
        let settings = self.reasoning.clone();
        let permit = self
            .reasoning_permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| ApiError(PlatformError::Unavailable))?;
        Ok((
            settings,
            (*self.config).clone(),
            (*self.namespace).clone(),
            permit,
        ))
    }
    pub(crate) fn git_policy(&self) -> nexora_platform_store::GitNetworkPolicy {
        (*self.git_policy).clone()
    }
    pub(crate) fn task_workflow_git_policy(&self) -> TaskWorkflowGitNetworkPolicy {
        (*self.task_workflow_git_policy).clone()
    }
    pub(crate) fn identity_policy(&self) -> nexora_platform_store::OidcNetworkPolicy {
        (*self.identity_policy).clone()
    }
    pub(crate) fn public_live_update_permit(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, ApiError> {
        self.public_live_permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| ApiError(PlatformError::Unavailable))
    }
    pub(crate) fn live_update_permit(&self) -> Result<tokio::sync::OwnedSemaphorePermit, ApiError> {
        self.live_permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| ApiError(PlatformError::Unavailable))
    }
    pub(crate) async fn run<T, F>(&self, action: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&mut PlatformStore) -> Result<T, PlatformError> + Send + 'static,
    {
        let permit = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.permits.clone().acquire_owned(),
        )
        .await
        .map_err(|_| ApiError(PlatformError::Unavailable))?
        .map_err(|_| ApiError(PlatformError::Unavailable))?;
        let config = self.config.clone();
        let namespace = self.namespace.clone();
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let available = pool
                .available
                .lock()
                .map_err(|_| PlatformError::Unavailable)?
                .pop();
            let mut store = match available {
                Some(store) => store,
                None => PlatformStore::connect(&config, &namespace)?,
            };
            let result = action(&mut store);
            if !matches!(&result, Err(PlatformError::Storage)) {
                pool.available
                    .lock()
                    .map_err(|_| PlatformError::Unavailable)?
                    .push(store);
            }
            result
        })
        .await
        .map_err(|_| ApiError(PlatformError::Unavailable))?
        .map_err(ApiError)
    }

    pub(crate) async fn run_task_workflow<T, F>(&self, action: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&mut TaskWorkflowStore) -> Result<T, PlatformError> + Send + 'static,
    {
        let permit = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.permits.clone().acquire_owned(),
        )
        .await
        .map_err(|_| ApiError(PlatformError::Unavailable))?
        .map_err(|_| ApiError(PlatformError::Unavailable))?;
        let config = self.config.clone();
        let namespace = self.namespace.clone();
        let pool = self.task_workflow_pool.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut store = match pool.take()? {
                Some(store) => store,
                None => TaskWorkflowStore::connect(config.as_ref(), &namespace)?,
            };
            let result = action(&mut store);
            if !matches!(&result, Err(PlatformError::Storage)) {
                pool.put(store)?;
            }
            result
        })
        .await
        .map_err(|_| ApiError(PlatformError::Unavailable))?
        .map_err(ApiError)
    }

    pub(crate) async fn task_transition_mutation<F>(
        &self,
        ctx: nexora_platform::AuthContext,
        operation: &'static str,
        request_id: String,
        input: serde_json::Value,
        action: F,
    ) -> Result<(Option<nexora_platform::WorkTask>, bool), ApiError>
    where
        F: FnOnce(
                &mut TaskWorkflowStore,
                &nexora_platform::AuthContext,
            ) -> Result<nexora_platform::WorkTask, PlatformError>
            + Send
            + 'static,
    {
        let result = self
            .run_task_workflow(move |store| {
                store.request(&ctx, operation, &request_id, &input, |store| {
                    action(store, &ctx)
                })
            })
            .await;
        match result {
            Ok(task) => {
                self.signal_job_wakeup();
                Ok((Some(task), false))
            }
            Err(ApiError(PlatformError::Conflict("waiting for knowledge worker"))) => {
                self.signal_job_wakeup();
                Ok((None, true))
            }
            Err(error) => Err(error),
        }
    }
    pub(crate) async fn mutate<T, F>(
        &self,
        ctx: nexora_platform::AuthContext,
        operation: &'static str,
        request_id: String,
        input: serde_json::Value,
        action: F,
    ) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&mut PlatformStore, &nexora_platform::AuthContext) -> Result<T, PlatformError>
            + Send
            + 'static,
    {
        let result = self
            .run(move |store| {
                store.request(&ctx, operation, &request_id, &input, |store| {
                    action(store, &ctx)
                })
            })
            .await;
        if result.is_ok() {
            self.signal_job_wakeup();
        }
        result
    }
}
