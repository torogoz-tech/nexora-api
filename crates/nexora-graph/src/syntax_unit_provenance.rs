use nexora_core::{LanguageId, RegionStatus, SourceSpan};
use serde::{Deserialize, Serialize};
/// Describes the token/CST/AST interpretation selected for a graph namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxUnitProvenance {
    pub unit_id: String,
    pub path: String,
    pub language: LanguageId,
    pub rule_version: String,
    pub profile_fingerprint: String,
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lexical_context: Option<String>,
    pub span: SourceSpan,
    pub status: RegionStatus,
    #[serde(default)]
    pub mutation_analysis: bool,
    #[serde(default)]
    pub direct_calls: bool,
}
