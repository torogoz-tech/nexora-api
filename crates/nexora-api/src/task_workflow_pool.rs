use nexora_task_workflow::TaskWorkflowStore;
use std::sync::Mutex;

/// Reuses task-workflow database sessions without caching authorization decisions.
pub(crate) struct TaskWorkflowPool {
    available: Mutex<Vec<TaskWorkflowStore>>,
}

impl TaskWorkflowPool {
    pub(crate) fn new(store: TaskWorkflowStore) -> Self {
        Self {
            available: Mutex::new(vec![store]),
        }
    }

    pub(crate) fn take(&self) -> Result<Option<TaskWorkflowStore>, nexora_platform::PlatformError> {
        self.available
            .lock()
            .map(|mut stores| stores.pop())
            .map_err(|_| nexora_platform::PlatformError::Unavailable)
    }

    pub(crate) fn put(
        &self,
        store: TaskWorkflowStore,
    ) -> Result<(), nexora_platform::PlatformError> {
        self.available
            .lock()
            .map(|mut stores| stores.push(store))
            .map_err(|_| nexora_platform::PlatformError::Unavailable)
    }
}

impl Drop for TaskWorkflowPool {
    fn drop(&mut self) {
        let stores = match self.available.get_mut() {
            Ok(stores) => std::mem::take(stores),
            Err(poisoned) => std::mem::take(poisoned.into_inner()),
        };
        if stores.is_empty() {
            return;
        }
        if tokio::runtime::Handle::try_current().is_ok() {
            std::thread::spawn(move || drop(stores));
        } else {
            drop(stores);
        }
    }
}
