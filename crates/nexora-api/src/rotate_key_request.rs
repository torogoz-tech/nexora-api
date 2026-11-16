use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RotateKeyRequest {
    pub expires_in_seconds: i64,
}
