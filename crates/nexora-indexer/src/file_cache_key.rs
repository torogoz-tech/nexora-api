use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileCacheKey {
    pub tenant: String,
    pub repository: String,
    pub configuration: String,
    pub engine: String,
    pub path: String,
    pub content_hash: String,
}
