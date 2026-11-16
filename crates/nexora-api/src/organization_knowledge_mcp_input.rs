use nexora_platform::OrganizationDocumentWrite;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationKnowledgeMcpInput {
    pub document_id: Option<String>,
    pub revision: Option<i64>,
    pub kind: Option<String>,
    pub search: Option<String>,
    pub after: Option<String>,
    pub request_id: Option<String>,
    pub document: Option<OrganizationDocumentWrite>,
}
