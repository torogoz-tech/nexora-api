use crate::api_error::ApiError;
use crate::store_pool::StorePool;
use nexora_platform::PlatformError;
use nexora_platform_store::PlatformStore;
use nexora_runtime::DatabaseConnections;
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
    identity_policy: Arc<nexora_platform_store::OidcNetworkPolicy>,
    config: Arc<DatabaseConnections>,
    namespace: Arc<String>,
    pool: Arc<StorePool>,
    permits: Arc<Semaphore>,
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
        let store = tokio::task::spawn_blocking(move || {
            PlatformStore::connect(&connect_config, &connect_namespace)
        })
        .await
        .map_err(|_| PlatformError::Unavailable)??;
        Ok(Self {
            public_live_permits: Arc::new(Semaphore::new(64)),
            live_permits: Arc::new(Semaphore::new(64)),
            reasoning: None,
            reasoning_permits: Arc::new(Semaphore::new(1)),
            git_policy: Arc::new(git_policy),
            identity_policy: Arc::new(policy),
            config: Arc::new(config),
            namespace: Arc::new(namespace),
            pool: Arc::new(StorePool::new(store)),
            permits: Arc::new(Semaphore::new(workers)),
        })
    }
    pub fn with_reasoning(mut self, settings: Option<crate::ReasoningSettings>) -> Self {
        self.reasoning = settings;
        self
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
        self.run(move |store| {
            store.request(&ctx, operation, &request_id, &input, |store| {
                action(store, &ctx)
            })
        })
        .await
    }
}
