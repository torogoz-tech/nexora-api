use nexora_platform::DocumentEvidence;
#[derive(Default)]
pub(crate) struct DocumentVerifiedContext {
    pub manifest_id: Option<String>,
    pub evidence: Vec<DocumentEvidence>,
}
