use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    pub tool: String,
    pub arguments: Value,
    pub sha256: String,
    pub result: Value,
}
