use nexora_core::{NexoraError, NodeId, ScopePolicy, SourceSpan};
use sha2::{Digest, Sha256};
pub struct SyntaxUnitIdentity;
impl SyntaxUnitIdentity {
    pub fn graph_owner(path: &str, unit_id: &str) -> NodeId {
        NodeId(format!("syntax-unit:{}:{path}:{unit_id}", path.len()))
    }
    pub fn for_interpretation(
        span: SourceSpan,
        language: &str,
        context: &str,
        lexical_context: Option<&str>,
        scope: &ScopePolicy,
        profile_fingerprint: &str,
    ) -> Result<String, NexoraError> {
        let identity = serde_json::to_vec(&(
            span,
            language,
            context,
            lexical_context,
            scope,
            profile_fingerprint,
        ))
        .map_err(|error| NexoraError::Configuration(error.to_string()))?;
        let digest = format!("{:x}", Sha256::digest(identity));
        Ok(if lexical_context.is_some() {
            format!("unit:lex:{digest}")
        } else {
            format!("unit:{digest}")
        })
    }
}
