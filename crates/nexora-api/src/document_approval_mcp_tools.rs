use crate::{ApiState, mcp_tool_result::McpToolResult};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;
pub(crate) struct DocumentApprovalMcpTools;
impl DocumentApprovalMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        if name != "nexora_document_approval_get" {
            return Err((-32602, "Unknown document approval tool"));
        }
        let input: crate::mcp_document_approval_get::McpDocumentApprovalGet =
            serde_json::from_value(args)
                .map_err(|_| (-32602, "Invalid document approval lookup"))?;
        let result = state
            .run(move |store| {
                serde_json::to_value(store.get_document_approval(
                    &ctx,
                    &input.project_id,
                    &input.document_id,
                    &input.version_id,
                    &input.approval_id,
                )?)
                .map_err(|_| PlatformError::Storage)
            })
            .await;
        McpToolResult::output(result, structured)
    }
}
