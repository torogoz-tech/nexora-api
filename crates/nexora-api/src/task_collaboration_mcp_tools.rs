use crate::{
    ApiState, mcp_task_comment_add::McpTaskCommentAdd, mcp_task_comments_list::McpTaskCommentsList,
    mcp_task_dependency_change::McpTaskDependencyChange,
    mcp_task_dependency_create::McpTaskDependencyCreate,
    mcp_task_dependency_list::McpTaskDependencyList, mcp_tool_result::McpToolResult,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct TaskCollaborationMcpTools;
impl TaskCollaborationMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        name: &str,
        args: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let invalid = |_| (-32602, "Invalid collaboration tool arguments");
        let result = match name {
            "nexora_task_commit_remove" => {
                let q: crate::mcp_task_commit::McpTaskCommit =
                    serde_json::from_value(args).map_err(invalid)?;
                let link = q.commit_id.ok_or((-32602, "commit_id required"))?;
                let revision = q
                    .input
                    .as_ref()
                    .and_then(|input| input["expected_revision"].as_i64())
                    .ok_or((-32602, "expected_revision required"))?;
                let canonical =
                    json!({"task_id":q.task_id,"commit_id":link,"expected_revision":revision});
                state
                    .mutate(
                        ctx,
                        "task.commit_remove",
                        q.request_id.ok_or((-32602, "request_id required"))?,
                        canonical,
                        move |store, ctx| {
                            store.remove_task_commit(ctx, &q.task_id, &link, revision)
                        },
                    )
                    .await
            }
            "nexora_task_commits_list" | "nexora_task_commit_bind" => {
                let q: crate::mcp_task_commit::McpTaskCommit =
                    serde_json::from_value(args).map_err(invalid)?;
                if name == "nexora_task_commits_list" {
                    state
                        .run(move |store| store.task_commits(&ctx, &q.task_id))
                        .await
                } else {
                    let input = q.input.ok_or((-32602, "input required"))?;
                    let canonical = json!({"task_id":q.task_id,"input":input});
                    state
                        .mutate(
                            ctx,
                            "task.commit_bind",
                            q.request_id.ok_or((-32602, "request_id required"))?,
                            canonical,
                            move |store, ctx| {
                                store.bind_task_commit(
                                    ctx,
                                    &q.task_id,
                                    &input,
                                    &nexora_platform_store::GitNetworkPolicy::default(),
                                )
                            },
                        )
                        .await
                }
            }
            "nexora_release_compose" | "nexora_release_composition_get" => {
                let q: crate::mcp_task_release::McpTaskRelease =
                    serde_json::from_value(args).map_err(invalid)?;
                let id = q.release_id.ok_or((-32602, "release_id required"))?;
                if name == "nexora_release_composition_get" {
                    state
                        .run(move |store| store.release_composition_jobs(&ctx, &q.project_id, &id))
                        .await
                } else {
                    let revision = q
                        .input
                        .as_ref()
                        .and_then(|input| input["expected_revision"].as_i64())
                        .ok_or((-32602, "release expected_revision required"))?;
                    let canonical = json!({"project_id":q.project_id,"release_id":id,"expected_revision":revision});
                    state
                        .mutate(
                            ctx,
                            "release.compose",
                            q.request_id.ok_or((-32602, "request_id required"))?,
                            canonical,
                            move |store, ctx| {
                                store.queue_release_composition(ctx, &q.project_id, &id, revision)
                            },
                        )
                        .await
                }
            }

            "nexora_releases_list"
            | "nexora_release_get"
            | "nexora_release_create"
            | "nexora_release_tasks_add" => {
                let q: crate::mcp_task_release::McpTaskRelease =
                    serde_json::from_value(args).map_err(invalid)?;
                match name {
                    "nexora_releases_list" => {
                        state
                            .run(move |store| store.task_releases(&ctx, &q.project_id))
                            .await
                    }
                    "nexora_release_get" => {
                        let id = q.release_id.ok_or((-32602, "release_id required"))?;
                        state
                            .run(move |store| store.task_release(&ctx, &q.project_id, &id))
                            .await
                    }
                    _ => {
                        if name == "nexora_release_create" && q.release_id.is_some() {
                            return Err((-32602, "release_id must be omitted for creation"));
                        }
                        if name == "nexora_release_tasks_add" && q.release_id.is_none() {
                            return Err((-32602, "release_id required"));
                        }
                        let input = q.input.ok_or((-32602, "input required"))?;
                        let operation = if q.release_id.is_some() {
                            "release.tasks_add"
                        } else {
                            "release.create"
                        };
                        let canonical = json!({"project_id":q.project_id,"release_id":q.release_id,"input":input});
                        state
                            .mutate(
                                ctx,
                                operation,
                                q.request_id.ok_or((-32602, "request_id required"))?,
                                canonical,
                                move |store, ctx| {
                                    store.save_task_release(
                                        ctx,
                                        &q.project_id,
                                        q.release_id.as_deref(),
                                        &input,
                                    )
                                },
                            )
                            .await
                    }
                }
            }

            "nexora_task_work_logs_list"
            | "nexora_task_work_log_add"
            | "nexora_task_work_log_change"
            | "nexora_task_work_log_remove" => {
                let q: crate::mcp_task_work_log::McpTaskWorkLog =
                    serde_json::from_value(args).map_err(invalid)?;
                if name == "nexora_task_work_logs_list" {
                    state
                        .run(move |store| {
                            serde_json::to_value(store.task_work_logs(&ctx, &q.task_id)?)
                                .map_err(|_| PlatformError::Storage)
                        })
                        .await
                } else {
                    let remove = name == "nexora_task_work_log_remove";
                    if name != "nexora_task_work_log_add" && q.work_log_id.is_none() {
                        return Err((-32602, "work_log_id is required"));
                    }
                    if name == "nexora_task_work_log_add" && q.work_log_id.is_some() {
                        return Err((-32602, "work_log_id must be omitted for creation"));
                    }
                    let input = q.input.ok_or((-32602, "input is required"))?;
                    let operation = if remove {
                        "task.work_log_removed"
                    } else if q.work_log_id.is_some() {
                        "task.work_log_changed"
                    } else {
                        "task.work_log_added"
                    };
                    let canonical = json!({"task_id":q.task_id,"work_log_id":q.work_log_id,"input":input,"remove":remove});
                    state
                        .mutate(
                            ctx,
                            operation,
                            q.request_id.ok_or((-32602, "request_id is required"))?,
                            canonical,
                            move |store, ctx| {
                                serde_json::to_value(store.save_task_work_log(
                                    ctx,
                                    &q.task_id,
                                    q.work_log_id.as_deref(),
                                    &input,
                                    remove,
                                )?)
                                .map_err(|_| PlatformError::Storage)
                            },
                        )
                        .await
                }
            }
            "nexora_task_project_change" => {
                let input: crate::mcp_task_project_change::McpTaskProjectChange =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"task_id":input.task_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "task.project.change",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.change_task_project(
                                ctx,
                                &input.task_id,
                                &input.change,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_knowledge_link" => {
                let input: crate::mcp_task_knowledge_link::McpTaskKnowledgeLink =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"task_id":input.task_id,"link":input.link});
                state
                    .mutate(
                        ctx,
                        if input.link.kind == "requirement" {
                            "document.link_create"
                        } else {
                            "task.knowledge.link"
                        },
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            store.link_task_knowledge(ctx, &input.task_id, &input.link)
                        },
                    )
                    .await
            }
            "nexora_task_stages_list" => {
                let input: crate::mcp_task_relations_get::McpTaskRelationsGet =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.task_stages(&ctx, &input.task_id)?)
                            .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_task_relations_get" => {
                let input: crate::mcp_task_relations_get::McpTaskRelationsGet =
                    serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| store.task_relations(&ctx, &input.task_id))
                    .await
            }
            "nexora_task_subtask_create" => {
                let input: crate::mcp_task_subtask_create::McpTaskSubtaskCreate =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"task_id":input.task_id,"subtask":input.subtask});
                state
                    .mutate(
                        ctx,
                        "task.subtask_create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.create_task_subtask(
                                ctx,
                                &input.task_id,
                                &input.subtask,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_subtask_link" | "nexora_task_subtask_remove" => {
                let input: crate::mcp_task_subtask_change::McpTaskSubtaskChange =
                    serde_json::from_value(args).map_err(invalid)?;
                let remove = name == "nexora_task_subtask_remove";
                let canonical = json!({"task_id":input.task_id,"child_task_id":input.child_task_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        if remove {
                            "task.subtask_remove"
                        } else {
                            "task.subtask_link"
                        },
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            store.change_task_subtask(
                                ctx,
                                &input.task_id,
                                &input.child_task_id,
                                &input.change,
                                remove,
                            )
                        },
                    )
                    .await
            }

            "nexora_task_dependency_create" => {
                let input: McpTaskDependencyCreate =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical =
                    json!({"project_id":input.project_id,"dependency":input.dependency});
                state
                    .mutate(
                        ctx,
                        "task.dependency_create",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(
                                if let Some(project) = input.project_id.as_deref() {
                                    store.create_task_dependency(ctx, project, &input.dependency)?
                                } else {
                                    store.create_pool_task_dependency(
                                        ctx,
                                        &input.dependency.source_task_id,
                                        &input.dependency,
                                    )?
                                },
                            )
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_dependency_resolve" => {
                let input: McpTaskDependencyChange =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"dependency_id":input.dependency_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "task.dependency_resolve",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.change_pool_dependency(
                                ctx,
                                input.task_id.as_deref(),
                                input.project_id.as_deref(),
                                &input.dependency_id,
                                &input.change,
                                false,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_dependency_remove" => {
                let input: McpTaskDependencyChange =
                    serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"dependency_id":input.dependency_id,"change":input.change});
                state
                    .mutate(
                        ctx,
                        "task.dependency_remove",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            serde_json::to_value(store.change_pool_dependency(
                                ctx,
                                input.task_id.as_deref(),
                                input.project_id.as_deref(),
                                &input.dependency_id,
                                &input.change,
                                true,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_dependencies_list" => {
                let input: McpTaskDependencyList = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        serde_json::to_value(store.pool_task_dependency_page(
                            &ctx,
                            input.project_id.as_deref(),
                            &input.task_id,
                            &input.query,
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_task_comment_add" => {
                let input: McpTaskCommentAdd = serde_json::from_value(args).map_err(invalid)?;
                let canonical = json!({"project_id":input.project_id,"task_id":input.task_id,"comment":input.comment});
                state
                    .mutate(
                        ctx,
                        "task.comment_add",
                        input.request_id,
                        canonical,
                        move |store, ctx| {
                            store.verify_task_project(
                                ctx,
                                &input.task_id,
                                input.project_id.as_deref(),
                            )?;
                            serde_json::to_value(store.add_pool_task_comment(
                                ctx,
                                &input.task_id,
                                &input.comment,
                            )?)
                            .map_err(|_| PlatformError::Storage)
                        },
                    )
                    .await
            }
            "nexora_task_comments_list" => {
                let input: McpTaskCommentsList = serde_json::from_value(args).map_err(invalid)?;
                state
                    .run(move |store| {
                        store.verify_task_project(
                            &ctx,
                            &input.task_id,
                            input.project_id.as_deref(),
                        )?;
                        serde_json::to_value(store.pool_task_comments(
                            &ctx,
                            &input.task_id,
                            input.after.as_deref(),
                            input.limit.unwrap_or(50),
                        )?)
                        .map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            _ => return Err((-32602, "Unknown collaboration tool")),
        };
        McpToolResult::output(result, structured)
    }
}
