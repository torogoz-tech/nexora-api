use crate::{ApiState, knowledge_mcp_input::KnowledgeMcpInput, mcp_tool_result::McpToolResult};
use nexora_platform::AuthContext;
use nexora_runtime::KnowledgeCommand;
use serde_json::{Value, json};

pub(crate) struct KnowledgeMcpTools;
impl KnowledgeMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let input = KnowledgeMcpInput::parse(name, args)?;
        let result = if let Some(command) = input.command {
            if command.is_write() {
                let operation = match command {
                    KnowledgeCommand::Contribute { .. } => "knowledge.contribute",
                    KnowledgeCommand::Revise { .. } => "knowledge.revise",
                    _ => "knowledge.review",
                };
                let request_id = command
                    .request_id()
                    .ok_or((-32602, "Knowledge request ID required"))?
                    .to_owned();
                let canonical = json!({"project_id":input.project_id,"repository_id":input.repository_id,"snapshot":input.snapshot,"command":command});
                state
                    .mutate(ctx, operation, request_id, canonical, move |store, ctx| {
                        store.query_repository_knowledge(
                            ctx,
                            &input.project_id,
                            &input.repository_id,
                            input.snapshot.as_deref(),
                            &command,
                        )
                    })
                    .await
            } else {
                state
                    .run(move |store| {
                        store.query_repository_knowledge(
                            &ctx,
                            &input.project_id,
                            &input.repository_id,
                            input.snapshot.as_deref(),
                            &command,
                        )
                    })
                    .await
            }
        } else {
            state
                .run(move |store| {
                    store.pending_repository_knowledge(
                        &ctx,
                        &input.project_id,
                        &input.repository_id,
                        input.snapshot.as_deref(),
                        input.offset,
                        input.limit,
                    )
                })
                .await
        };
        McpToolResult::output(result, structured)
    }
}
