use crate::{ApiState, mcp_tool_result::McpToolResult};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct TaskLifecycleMcpTools;
impl TaskLifecycleMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid task lifecycle arguments");
        let result = match name {
            "nexora_task_pool_list" => {
                let input: crate::mcp_task_pool_list::McpTaskPoolList =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.visible_tasks(&ctx, &input.query)?)
                            .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_task_pool_options" => {
                let input: crate::mcp_task_pool_options::McpTaskPoolOptions =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| store.task_pool_options(&ctx, input.project_id.as_deref()))
                    .await
            }
            "nexora_task_general_complete" => {
                let input: crate::mcp_task_pause::McpTaskPause =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical =
                    json!({"task_id":input.task_id,"expected_revision":input.expected_revision});
                state
                    .mutate(
                        ctx,
                        "task.general_complete",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.complete_general_task(
                                ctx,
                                &input.task_id,
                                input.expected_revision,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_pause" | "nexora_task_resume" => {
                let input: crate::mcp_task_pause::McpTaskPause =
                    serde_json::from_value(args).map_err(invalid)?;
                let paused = name == "nexora_task_pause";
                let operation = if paused { "task.pause" } else { "task.resume" };
                let canonical =
                    json!({"task_id":input.task_id,"expected_revision":input.expected_revision});
                state
                    .mutate(
                        ctx,
                        operation,
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.set_task_paused(
                                ctx,
                                &input.task_id,
                                input.expected_revision,
                                paused,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_take" => {
                let input: crate::mcp_task_take::McpTaskTake =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"task_id":input.task_id,"take":input.take});
                let policy = state.git_policy();
                state
                    .mutate(
                        ctx,
                        "task.take",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.take_task_with_repositories(
                                ctx,
                                &input.task_id,
                                &input.take,
                                input.project_id.as_deref(),
                                &policy,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }

            "nexora_task_move" => {
                let input: crate::mcp_task_move::McpTaskMove =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"task_id":input.task_id,"movement":input.movement});
                state
                    .mutate(
                        ctx,
                        "task.move",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.move_task(
                                ctx,
                                &input.project_id,
                                &input.task_id,
                                &input.movement,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_workflow_migrate" => {
                let input: crate::mcp_task_workflow_migrate::McpTaskWorkflowMigrate =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"task_id":input.task_id,"migration":input.migration});
                state
                    .mutate(
                        ctx,
                        "task.workflow_migrate",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.migrate_task_workflow(
                                ctx,
                                &input.project_id,
                                &input.task_id,
                                &input.migration,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            _ => return Err((-32602, "Unknown task lifecycle tool")),
        };
        McpToolResult::output(result, structured)
    }
}
