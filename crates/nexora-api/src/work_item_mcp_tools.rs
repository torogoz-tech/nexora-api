use crate::{
    ApiState, mcp_backlog_change::McpBacklogChange, mcp_backlog_create::McpBacklogCreate,
    mcp_backlog_get::McpBacklogGet, mcp_backlog_reorder::McpBacklogReorder,
    mcp_task_approval_get::McpTaskApprovalGet, mcp_task_change::McpTaskChange,
    mcp_task_create::McpTaskCreate, mcp_task_get::McpTaskGet, mcp_task_history::McpTaskHistory,
    mcp_task_list::McpTaskList, mcp_task_transition::McpTaskTransition,
    mcp_task_type_create::McpTaskTypeCreate, mcp_tool_result::McpToolResult,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct WorkItemMcpTools;
impl WorkItemMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid work-item tool arguments");
        let result = match name {
            "nexora_task_transition_runs" => {
                let input: McpTaskGet = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        store.verify_task_project(
                            &ctx,
                            &input.task_id,
                            input.project_id.as_deref(),
                        )?;
                        store.task_transition_runs(&ctx, &input.task_id)
                    })
                    .await
            }
            "nexora_task_transition_retry" => {
                let input: crate::mcp_task_transition_retry::McpTaskTransitionRetry =
                    serde_json::from_value(args).map_err(invalid)?;
                let policy = state.git_policy();
                state
                    .mutate(
                        ctx,
                        "task.transition_retry",
                        input.request_id,
                        json!({"task_id":input.task_id,"run_id":input.run_id}),
                        move |store, ctx| {
                            store.verify_task_project(
                                ctx,
                                &input.task_id,
                                input.project_id.as_deref(),
                            )?;
                            serde_json::to_value(store.retry_task_transition(
                                ctx,
                                &input.task_id,
                                &input.run_id,
                                &policy,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_approval_get" => {
                let input: McpTaskApprovalGet = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        store.verify_task_project(
                            &ctx,
                            &input.task_id,
                            input.project_id.as_deref(),
                        )?;
                        serde_json::to_value(store.get_pool_task_approval(
                            &ctx,
                            &input.task_id,
                            &input.approval_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }

            "nexora_task_type_create" => {
                let input: McpTaskTypeCreate = serde_json::from_value(args).map_err(invalid)?;
                let canonical =
                    json!({"project_id":input.project_id,"definition":input.definition});
                state
                    .mutate(
                        ctx,
                        "task_type.create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.create_task_type(
                                ctx,
                                &input.project_id,
                                &input.definition,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_backlog_create" => {
                let input: McpBacklogCreate = serde_json::from_value(args).map_err(invalid)?;
                let canonical =
                    json!({"project_id":input.project_id,"definition":input.definition});
                state
                    .mutate(
                        ctx,
                        "backlog.create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.create_backlog(
                                ctx,
                                &input.project_id,
                                &input.definition,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_backlog_change" => {
                let input: McpBacklogChange = serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"backlog_id":input.backlog_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "backlog.change",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.change_backlog(
                                ctx,
                                &input.project_id,
                                &input.backlog_id,
                                &input.change,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_backlog_reorder" => {
                let input: McpBacklogReorder = serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"backlog_id":input.backlog_id,"order":input.order});
                state
                    .mutate(
                        ctx,
                        "backlog.reorder",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.reorder_backlog(
                                ctx,
                                &input.project_id,
                                &input.backlog_id,
                                &input.order,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_create" => {
                let input: McpTaskCreate = serde_json::from_value(args).map_err(invalid)?;
                let canonical =
                    json!({"project_id":input.project_id,"definition":input.definition});
                state
                    .mutate(
                        ctx,
                        "task.create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.create_pool_task(
                                ctx,
                                &nexora_platform::PoolTaskCreate {
                                    project_id: input.project_id,
                                    definition: input.definition,
                                },
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_change" => {
                let input: McpTaskChange = serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"task_id":input.task_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "task.change",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            store.verify_task_project(
                                ctx,
                                &input.task_id,
                                input.project_id.as_deref(),
                            )?;
                            serde_json::to_value(store.change_pool_task(
                                ctx,
                                &input.task_id,
                                &input.change,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_transition" => {
                let input: McpTaskTransition = serde_json::from_value(args).map_err(invalid)?;
                let policy = state.git_policy();
                let canonical = json!({"project_id":input.project_id,"task_id":input.task_id,"transition_id":input.transition_id,"parameters":input.parameters});
                state
                    .mutate(
                        ctx,
                        "task.transition",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            store.verify_task_project(
                                ctx,
                                &input.task_id,
                                input.project_id.as_deref(),
                            )?;
                            serde_json::to_value(store.transition_pool_task_with_policy(
                                ctx,
                                &input.task_id,
                                &input.transition_id,
                                &input.parameters,
                                &policy,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_get" => {
                let input: McpTaskGet = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        store.verify_task_project(
                            &ctx,
                            &input.task_id,
                            input.project_id.as_deref(),
                        )?;
                        store.pool_task_presentation(&ctx, &input.task_id)
                    })
                    .await
            }
            "nexora_backlog_get" => {
                let input: McpBacklogGet = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.get_backlog(
                            &ctx,
                            &input.project_id,
                            &input.backlog_id,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_tasks_list" => {
                let input: McpTaskList = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        if let Some(project)=input.project_id.as_deref() {
                            serde_json::to_value(store.task_page(&ctx,project,&input.query)?)
                        } else {
                            if input.query.assignee_id.is_some() || input.query.backlog_id.is_some() || input.query.priority.is_some() || input.query.state_id.is_some() || input.query.workflow_version_id.is_some() || input.query.include_archived {return Err(PlatformError::Validation("use a project scope for detailed task filters or nexora_task_pool_list for pool filters"));}
                            let query=nexora_platform::VisibleTaskQuery {type_code:input.query.type_code,after:input.query.after,limit:input.query.limit,..Default::default()};
                            serde_json::to_value(store.visible_tasks(&ctx,&query)?)
                        }
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_task_history" => {
                let input: McpTaskHistory = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        store.verify_task_project(
                            &ctx,
                            &input.task_id,
                            input.project_id.as_deref(),
                        )?;
                        serde_json::to_value(store.pool_task_history(
                            &ctx,
                            &input.task_id,
                            input.after,
                            input.limit.unwrap_or(50),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            _ => return Err((-32602, "Unknown work-item tool")),
        };
        McpToolResult::output(result, structured)
    }
}
