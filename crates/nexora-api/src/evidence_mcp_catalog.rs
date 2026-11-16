use serde_json::Value;
pub(crate) struct EvidenceMcpCatalog;
impl EvidenceMcpCatalog {
    pub fn list() -> Vec<Value> {
        serde_json::from_str(include_str!("../schemas/evidence_tools.json"))
            .expect("bundled evidence tools")
    }
}
