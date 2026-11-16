use nexora_core::{RepositoryId, TenantId};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationReference {
    pub tenant_id: TenantId,
    pub repository_id: RepositoryId,
    pub evaluation_id: String,
}
