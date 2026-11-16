use nexora_platform::KeyScope;
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueKeyRequest {
    pub name: String,
    pub scope: KeyScope,
    pub expires_in_seconds: i64,
}
