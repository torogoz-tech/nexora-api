use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnalysisProfileQuery {
    pub after: Option<String>,
    #[serde(default = "AnalysisProfileQuery::limit")]
    pub limit: u16,
}
impl AnalysisProfileQuery {
    fn limit() -> u16 {
        50
    }
}
