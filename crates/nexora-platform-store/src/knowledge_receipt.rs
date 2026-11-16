use crate::repository_snapshot_scope::RepositorySnapshotScope;
use nexora_platform::{AuthContext, PlatformError};
use nexora_runtime::{DatabaseRuntime, QuerySnapshot};
use postgres::Transaction;
use serde_json::{Value, json};

pub(crate) struct KnowledgeReceipt;
impl KnowledgeReceipt {
    pub fn encode(authority: &Value, head: &QuerySnapshot, response: &Value) -> Value {
        json!({"version":1,"pin":{"authority":authority,"snapshot":head.id.0,"payload_hash":head.payload_hash},"response":response})
    }
    #[allow(clippy::too_many_arguments)]
    pub fn replay(
        tx: &mut Transaction<'_>,
        runtime: &mut DatabaseRuntime,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        current_authority: &Value,
        receipt: Value,
    ) -> Result<Value, PlatformError> {
        if receipt["version"] != 1 || &receipt["pin"]["authority"] != current_authority {
            return Err(PlatformError::NotFound);
        }
        let snapshot = receipt["pin"]["snapshot"]
            .as_str()
            .ok_or(PlatformError::Storage)?;
        let original = RepositorySnapshotScope::lock(tx, ctx, project, repository, Some(snapshot))?;
        let head = original.verify(runtime)?;
        if receipt["pin"]["payload_hash"] != head.payload_hash {
            return Err(PlatformError::Conflict(
                "knowledge receipt snapshot checksum changed",
            ));
        }
        let response = receipt
            .get("response")
            .filter(|v| v.is_object())
            .ok_or(PlatformError::Storage)?;
        Ok(response.clone())
    }
}
