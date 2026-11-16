use crate::{
    ApiState, mcp_project_change::McpProjectChange, mcp_project_create::McpProjectCreate,
    mcp_tool_result::McpToolResult,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};

pub(crate) struct ProjectMutationMcpTools;
impl ProjectMutationMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid project mutation arguments");
        let result = match name {
            "nexora_project_create" => {
                let input: McpProjectCreate = serde_json::from_value(args).map_err(invalid)?;
                let canonical = serde_json::to_value(&input.definition)
                    .map_err(|_| (-32602, "Invalid project definition"))?;
                state
                    .mutate(
                        ctx,
                        "project.create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.create_project_with_color(
                                ctx,
                                &input.definition.name,
                                input.definition.description.as_deref(),
                                input.definition.color.as_ref(),
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_project_change" => {
                let input: McpProjectChange = serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"id":input.project_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "project.change",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.change_project(
                                ctx,
                                &input.project_id,
                                &input.change,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            _ => return Err((-32602, "Unknown project mutation tool")),
        };
        McpToolResult::output(result, structured)
    }
}
