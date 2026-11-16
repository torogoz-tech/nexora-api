pub struct MultiProfileIndexResult {
    pub index: crate::IncrementalSnapshot,
    /// Safe provenance: no model package bodies, credentials, or absolute paths.
    pub selection: serde_json::Value,
}
