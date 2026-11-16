use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageTimings {
    pub preparation_ms: f64,
    pub read_ms: f64,
    pub lexer_ms: f64,
    pub parser_ms: f64,
    pub ast_ms: f64,
    pub symbols_ms: f64,
    pub graph_ms: f64,
    #[serde(default)]
    pub composition_ms: f64,
    pub resolution_ms: f64,
    pub publication_ms: f64,
    pub total_ms: f64,
}
