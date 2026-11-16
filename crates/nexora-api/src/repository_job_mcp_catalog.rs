use serde_json::{Value, json};

pub(crate) struct RepositoryJobMcpCatalog;
impl RepositoryJobMcpCatalog {
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_sync_repository"
                | "nexora_index_repository"
                | "nexora_repository_job_get"
                | "nexora_repository_job_cancel"
        )
    }
    pub fn list() -> Vec<Value> {
        let id = json!({"type":"string","minLength":1,"maxLength":128});
        let request_id = json!({"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9._:-]+$","description":"Actor-scoped idempotency key. Preserve for exact retries; credential prefixes nxk_ and nxs_ are forbidden."});
        let request = json!({"type":"object","additionalProperties":false,"required":["expected_revision"],"properties":{"expected_revision":{"type":"integer","minimum":1}}});
        let mut tools = Vec::new();
        for (name, description) in [
            (
                "nexora_sync_repository",
                "Enqueue a durable sync job for a registered, authorized Git repository (local snapshots are not applicable) at its expected revision. Returns job metadata, not a completed snapshot. Source authority is checked again by the worker. Exact retries return the original acknowledged job.",
            ),
            (
                "nexora_index_repository",
                "Enqueue a durable indexing job for a registered, authorized Git repository (local snapshots are not applicable) at its expected revision. No arbitrary root, tenant or actor is accepted. Returns job metadata; poll the job before querying a successfully published snapshot. Exact retries return the original acknowledged job.",
            ),
        ] {
            tools.push(json!({"name":name,"description":description,"inputSchema":{"type":"object","additionalProperties":false,"required":["project_id","repository_id","request_id","request"],"properties":{"project_id":id,"repository_id":id,"request_id":request_id,"request":request}},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":true}}));
        }
        tools.push(json!({"name":"nexora_repository_job_get","description":"Read current durable job metadata in the authorized project, including terminal history. Does not grant code access or return worker lease tokens/provider credentials.","inputSchema":{"type":"object","additionalProperties":false,"required":["project_id","job_id"],"properties":{"project_id":id,"job_id":id}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}));
        tools.push(json!({"name":"nexora_repository_job_cancel","description":"Cancel a queued/running job with its expected revision. Does not undo a snapshot already published; terminal jobs reject a new cancellation. Preserve request_id to replay an acknowledged cancellation exactly.","inputSchema":{"type":"object","additionalProperties":false,"required":["project_id","job_id","request_id","change"],"properties":{"project_id":id,"job_id":id,"request_id":request_id,"change":request}},"annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":true,"openWorldHint":false}}));
        tools
    }
}
