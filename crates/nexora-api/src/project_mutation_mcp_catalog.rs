use serde_json::{Value, json};
pub(crate) struct ProjectMutationMcpCatalog;
impl ProjectMutationMcpCatalog {
    pub fn list() -> Vec<Value> {
        let id = json!({"type":"string","minLength":1,"maxLength":128});
        let name = json!({"type":"string","minLength":1,"maxLength":160});
        let description =
            json!({"anyOf":[{"type":"string","minLength":1,"maxLength":4000},{"type":"null"}]});
        let color = json!({"anyOf":[{"type":"string","pattern":"^#[0-9A-Fa-f]{6}$"},{"type":"null"}],"description":"Optional display RGB. Omit on change to preserve; null clears."});
        vec![
            json!({"name":"nexora_project_create","description":"Create a project with optional display color. Requires installation projects:write; stable request_id enables exact retry.","inputSchema":{"type":"object","additionalProperties":false,"required":["request_id","definition"],"properties":{"request_id":id,"definition":{"type":"object","additionalProperties":false,"required":["name"],"properties":{"name":name,"description":description,"color":color}}}},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
            json!({"name":"nexora_project_change","description":"Replace editable project fields with expected_revision. Omitted color preserves it; null clears it. Requires projects:write.","inputSchema":{"type":"object","additionalProperties":false,"required":["project_id","request_id","change"],"properties":{"project_id":id,"request_id":id,"change":{"type":"object","additionalProperties":false,"required":["expected_revision","name","archived"],"properties":{"expected_revision":{"type":"integer","minimum":1},"name":name,"description":description,"archived":{"type":"boolean"},"color":color}}}},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
        ]
    }
}
