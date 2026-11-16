use serde_json::Value;
/// Only a newly persisted claim carries model input; retries never do.
pub struct PreparedCodeReview {
    pub review: Value,
    pub input: Option<Value>,
}
