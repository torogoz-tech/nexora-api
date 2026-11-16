use std::collections::BTreeMap;
#[derive(Default)]
pub(crate) struct RequestRegistry {
    active: BTreeMap<String, bool>,
}
impl RequestRegistry {
    pub fn contains(&self, key: &str) -> bool {
        self.active.contains_key(key)
    }
    pub fn register(&mut self, key: &str) -> Result<(), &'static str> {
        if self.active.contains_key(key) {
            return Err("duplicate active request id");
        }
        if self.active.len() >= 8 {
            return Err("server busy: maximum eight tool calls in flight");
        }
        self.active.insert(key.into(), false);
        Ok(())
    }
    pub fn cancel(&mut self, key: &str) {
        if let Some(cancelled) = self.active.get_mut(key) {
            *cancelled = true;
        }
    }
    pub fn cancelled(&self, key: &str) -> bool {
        self.active.get(key).copied().unwrap_or(true)
    }
    pub fn complete(&mut self, key: &str) -> bool {
        self.active.remove(key).unwrap_or(true)
    }
}
