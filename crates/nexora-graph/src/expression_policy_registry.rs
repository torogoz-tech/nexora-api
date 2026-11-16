use nexora_core::NexoraError;
use nexora_language::ExpressionBindingDefinition;
use sha2::{Digest, Sha256};
/// Content identity permits one immutable policy record per graph/snapshot.
pub struct ExpressionPolicyRegistry;
impl ExpressionPolicyRegistry {
    pub fn id(policy: &ExpressionBindingDefinition) -> Result<String, NexoraError> {
        let bytes =
            serde_json::to_vec(policy).map_err(|e| NexoraError::Configuration(e.to_string()))?;
        Ok(format!("expression-policy:{:x}", Sha256::digest(bytes)))
    }
}
