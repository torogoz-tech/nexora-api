use nexora_language::AnalysisProfilePackage;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnalysisProfileEvolutionInput {
    pub request_id: Option<String>,
    pub package: Option<AnalysisProfilePackage>,
    pub profile_id: Option<String>,
    pub project_id: Option<String>,
    pub repository_id: Option<String>,
    pub after: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<u16>,
}
