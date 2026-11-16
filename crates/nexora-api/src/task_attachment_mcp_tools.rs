use crate::{
    ApiState, api_error::ApiError, mcp_task_attachment_upload::McpTaskAttachmentUpload,
    mcp_task_attachments_query::McpTaskAttachmentsQuery, mcp_tool_result::McpToolResult,
    task_evidence_image::TaskEvidenceImage,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(crate) struct TaskAttachmentMcpTools;
impl TaskAttachmentMcpTools {
    pub fn list() -> Vec<Value> {
        serde_json::from_str(include_str!("../schemas/task_attachment_tools.json"))
            .expect("task attachment tool schemas")
    }
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_task_image_upload"
                | "nexora_task_attachment_upload"
                | "nexora_task_attachments_list"
        )
    }
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        if name == "nexora_task_attachments_list" {
            let q: McpTaskAttachmentsQuery = serde_json::from_value(args)
                .map_err(|_| (-32602, "Invalid task attachment query"))?;
            return McpToolResult::output(
                state
                    .run(move |s| {
                        s.task_document_page(&ctx, Some(&q.task_id), None, "", q.after.as_deref())
                    })
                    .await,
                structured,
            );
        }
        let q: McpTaskAttachmentUpload =
            serde_json::from_value(args).map_err(|_| (-32602, "Invalid task attachment upload"))?;
        if q.data_base64.len() > 27962028 {
            return McpToolResult::output(
                Err(ApiError(PlatformError::Validation(
                    "document exceeds 20 MiB",
                ))),
                structured,
            );
        }
        let image = name == "nexora_task_image_upload";
        let bytes = if image {
            TaskEvidenceImage::normalize(&q.data_base64)
        } else {
            STANDARD
                .decode(&q.data_base64)
                .map_err(|_| PlatformError::Validation("invalid base64"))
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(error) => return McpToolResult::output(Err(ApiError(error)), structured),
        };
        let canonical = json!({"task_id":q.task_id,"name":q.name,"sha256":format!("{:x}",Sha256::digest(&bytes))});
        let result = state
            .mutate(
                ctx,
                if image {
                    "task.image.upload"
                } else {
                    "task.document.upload"
                },
                q.request_id,
                canonical,
                move |s, c| {
                    if image {
                        s.upload_task_image(c, &q.task_id, &q.name, &bytes)
                    } else {
                        s.upload_task_document(c, &q.task_id, &q.name, &bytes)
                    }
                },
            )
            .await;
        McpToolResult::output(result, structured)
    }
}
