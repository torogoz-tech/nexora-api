use crate::{
    ApiState, mcp_tool_result::McpToolResult, mcp_workflow_change::McpWorkflowChange,
    mcp_workflow_create::McpWorkflowCreate, mcp_workflow_list::McpWorkflowList,
    mcp_workflow_publish::McpWorkflowPublish, mcp_workflow_query::McpWorkflowQuery,
    mcp_workflow_version_query::McpWorkflowVersionQuery,
    mcp_workflow_versions_query::McpWorkflowVersionsQuery,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};

pub(crate) struct WorkflowMcpTools;
impl WorkflowMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid workflow tool arguments");
        let result = match name {
            "nexora_workflows_list" => {
                let input: McpWorkflowList = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.developer_workflow_page(
                            &ctx,
                            input.after.as_deref(),
                            input.limit.unwrap_or(50),
                            input.include_archived.unwrap_or(false),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_workflow_get" => {
                let input: McpWorkflowQuery = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.developer_workflow(&ctx, &input.workflow_id)?)
                            .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_workflow_version_get" => {
                let input: McpWorkflowVersionQuery =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.developer_workflow_version(
                            &ctx,
                            &input.workflow_id,
                            &input.version_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_workflow_versions_list" => {
                let input: McpWorkflowVersionsQuery =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.developer_workflow_versions(
                            &ctx,
                            &input.workflow_id,
                            input.before,
                            input.limit.unwrap_or(50),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_workflow_create" => {
                let input: McpWorkflowCreate = serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"definition":input.definition});
                state
                    .mutate(
                        ctx,
                        "workflow.create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(
                                store.create_developer_workflow(ctx, &input.definition)?,
                            )
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_workflow_change" => {
                let input: McpWorkflowChange = serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"workflow_id":input.workflow_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "workflow.change",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.change_developer_workflow(
                                ctx,
                                &input.workflow_id,
                                &input.change,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_workflow_publish" => {
                let input: McpWorkflowPublish = serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"workflow_id":input.workflow_id,"publish":input.publish});
                state
                    .mutate(
                        ctx,
                        "workflow.publish",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.publish_developer_workflow(
                                ctx,
                                &input.workflow_id,
                                &input.publish,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_workflow_developers_list" => {
                let input: McpWorkflowQuery = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(
                            store.developer_workflow_members(&ctx, &input.workflow_id)?,
                        )
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_developer_workflows_list" => {
                let input: crate::mcp_developer_workflow_query::McpDeveloperWorkflowQuery =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        let actor = input.actor_id.as_deref().unwrap_or(ctx.actor_id());
                        serde_json::to_value(store.actor_workflows(&ctx, actor)?)
                            .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_developer_workflow_get" => {
                let input: crate::mcp_developer_workflow_query::McpDeveloperWorkflowQuery =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        let actor = input.actor_id.as_deref().unwrap_or(ctx.actor_id());
                        serde_json::to_value(store.actor_workflow(&ctx, actor)?)
                            .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_developer_workflow_assign" => {
                let input: crate::mcp_developer_workflow_assign::McpDeveloperWorkflowAssign =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"actor_id":input.actor_id,"assignment":input.assignment});
                state
                    .mutate(
                        ctx,
                        "developer.workflow_assign",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.assign_actor_workflow(
                                ctx,
                                &input.actor_id,
                                &input.assignment,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_developer_tasks_list" => {
                let input: McpWorkflowList = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.developer_board_tasks(
                            &ctx,
                            input.after.as_deref(),
                            input.limit.unwrap_or(100),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            _ => return Err((-32602, "Unknown workflow tool")),
        };
        McpToolResult::output(result, structured)
    }
}
