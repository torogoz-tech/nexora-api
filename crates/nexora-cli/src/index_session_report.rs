use serde::Serialize;
#[derive(Serialize)]
pub struct IndexSessionReport {
    pub schema_version: u32,
    pub analysis_engine: String,
    pub passes: Vec<crate::index_session_pass::IndexSessionPass>,
    pub final_snapshot: nexora_storage::Snapshot,
}
