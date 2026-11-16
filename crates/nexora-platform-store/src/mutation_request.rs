#[derive(Clone)]
pub(crate) struct MutationRequest {
    pub actor_id: String,
    pub operation: String,
    pub request_id: String,
    pub fingerprint: String,
}
