use nexora_platform_store::PlatformStore;
use std::sync::Mutex;
/// Owns connection shutdown so synchronous clients never close on an async worker.
pub(crate) struct StorePool {
    pub available: Mutex<Vec<PlatformStore>>,
}
impl StorePool {
    pub fn new(store: PlatformStore) -> Self {
        Self {
            available: Mutex::new(vec![store]),
        }
    }
}
impl Drop for StorePool {
    fn drop(&mut self) {
        let available = match self.available.get_mut() {
            Ok(stores) => stores,
            Err(poisoned) => poisoned.into_inner(),
        };
        let stores = std::mem::take(available);
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
