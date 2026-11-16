use serde_json::{Value, json};
pub(crate) struct DocumentApprovalMcpCatalog;
impl DocumentApprovalMcpCatalog {
    pub fn list() -> Vec<Value> {
        vec![
            json!({"name":"nexora_document_approval_get","description":"Inspect a human authorization for an exact document version, including consumption or revocation. Requires document-read authority. Granting and revoking require human login sessions through the API.","inputSchema":{"type":"object","additionalProperties":false,"required":["project_id","document_id","version_id","approval_id"],"properties":{"project_id":{"type":"string","minLength":1,"maxLength":128},"document_id":{"type":"string","minLength":1,"maxLength":128},"version_id":{"type":"string","minLength":1,"maxLength":128},"approval_id":{"type":"string","minLength":1,"maxLength":128}}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
        ]
    }
    pub fn supports(name: &str) -> bool {
        name == "nexora_document_approval_get"
    }
}
