use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaiverRequest {
    pub finding_id: String,
    pub policy_id: String,
    pub policy_version: crate::PolicyVersion,
    pub tenant_id: nexora_core::TenantId,
    pub repository_id: nexora_core::RepositoryId,
    pub reason: String,
    pub expires_at: String,
    pub authorized_by: String,
}
