use serde_json::{Value, json};
pub(crate) struct GitAuthorMcpCatalog;
impl GitAuthorMcpCatalog {
    pub fn list() -> Vec<Value> {
        ["nexora_git_authors_list","nexora_git_author_aliases_list"].iter().map(|name|json!({"name":name,"description":"Read authorized received Git authorship or administrative author aliases. Alias attribution does not cryptographically authenticate a Git author. Human-only mapping changes are not available to keys.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"query":{"type":"object","additionalProperties":false,"properties":{"project_id":{"type":["string","null"]},"member_id":{"type":["string","null"]},"search":{"type":["string","null"]},"after":{"type":["string","null"]},"limit":{"type":"integer","minimum":1,"maximum":100}}}}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})).collect()
    }
}
