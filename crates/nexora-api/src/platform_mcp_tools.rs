use crate::{
    ApiState, api_error::ApiError, mcp_project_query::McpProjectQuery,
    project_list_query::ProjectListQuery,
};
use crate::{mcp_tool_result::McpToolResult, repository_mcp_tools::RepositoryMcpTools};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
pub(crate) struct PlatformMcpTools;
impl PlatformMcpTools {
    pub async fn call(
        state: ApiState,
        ctx: AuthContext,
        params: Value,
        structured: bool,
    ) -> Result<Value, (i64, &'static str)> {
        let object = params
            .as_object()
            .ok_or((-32602, "Tool parameters must be an object"))?;
        if object
            .keys()
            .any(|k| !matches!(k.as_str(), "name" | "arguments" | "_meta"))
        {
            return Err((-32602, "Unknown tool parameter"));
        }
        let name = params["name"]
            .as_str()
            .ok_or((-32602, "Tool name required"))?;
        let authorization = ctx.clone();
        let tool = name.to_owned();
        if let Err(error) = state
            .run(move |s| s.authorize_request_tool(&authorization, &tool))
            .await
        {
            return McpToolResult::output(Err(error), structured);
        }
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        let result: Result<Value, ApiError> = match name {
            n if (n.starts_with("nexora_knowledge_review_") || n == "nexora_products_list") => {
                return crate::knowledge_review_mcp_tools::KnowledgeReviewMcpTools::call(
                    state, ctx, n, args, structured,
                )
                .await;
            }
            n if n.starts_with("nexora_request_") => {
                return crate::service_request_mcp_tools::ServiceRequestMcpTools::call(
                    state, ctx, n, args, structured,
                )
                .await;
            }
            n if crate::task_attachment_mcp_tools::TaskAttachmentMcpTools::supports(n) => {
                return crate::task_attachment_mcp_tools::TaskAttachmentMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            n if crate::task_work_mcp_tools::TaskWorkMcpTools::supports(n) => {
                return crate::task_work_mcp_tools::TaskWorkMcpTools::call(
                    state, ctx, n, args, structured,
                )
                .await;
            }
            "nexora_knowledge_context_search"
            | "nexora_knowledge_contribute"
            | "nexora_knowledge_contribution_get"
            | "nexora_knowledge_contributions_list" => {
                return crate::knowledge_contribution_mcp::KnowledgeContributionMcp::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            "nexora_features_list" | "nexora_feature_map_propose" | "nexora_feature_complete" => {
                return crate::feature_mcp_tools::FeatureMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            "nexora_evidence_upload" | "nexora_evidence_get" | "nexora_evidence_list" => {
                return crate::evidence_mcp_tools::EvidenceMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            "nexora_evaluate_repository" => {
                return crate::reasoning_mcp_tools::ReasoningMcpTools::call(
                    state, ctx, args, structured,
                )
                .await;
            }
            "nexora_project_create" | "nexora_project_change" => {
                return crate::project_mutation_mcp_tools::ProjectMutationMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            "nexora_git_authors_list" | "nexora_git_author_aliases_list" => {
                let input: crate::mcp_git_author_query::McpGitAuthorQuery =
                    serde_json::from_value(args)
                        .map_err(|_| (-32602, "Invalid Git author query"))?;
                let aliases = name == "nexora_git_author_aliases_list";
                state
                    .run(move |store| {
                        if aliases {
                            store.git_author_aliases(&ctx, &input.query)
                        } else {
                            store.git_authors(&ctx, &input.query)
                        }
                    })
                    .await
            }
            "nexora_analysis_model_guide_get"
            | "nexora_analysis_profile_validate"
            | "nexora_analysis_profile_package" => {
                return crate::analysis_profile_authoring_mcp_tools::AnalysisProfileAuthoringMcpTools::call(state,ctx,name,args,structured).await;
            }
            "nexora_analysis_profiles_list"
            | "nexora_analysis_profile_get"
            | "nexora_analysis_profile_install"
            | "nexora_repository_analysis_issues" => {
                return crate::analysis_profile_evolution_mcp::AnalysisProfileEvolutionMcp::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            "nexora_repository_analysis_profile_get" => {
                return crate::repository_analysis_profile_mcp_tools::RepositoryAnalysisProfileMcpTools::call(state, ctx, args, structured).await;
            }
            "nexora_permissions_get" => {
                if !args.as_object().is_some_and(|v| v.is_empty()) {
                    return Err((-32602, "Permissions lookup takes no arguments"));
                }
                state
                    .run(move |store| store.effective_permissions(&ctx))
                    .await
            }
            "nexora_projects_list" => {
                let query: ProjectListQuery = serde_json::from_value(args)
                    .map_err(|_| (-32602, "Invalid project list arguments"))?;
                state
                    .run(move |store| {
                        let page = store.project_page(
                            &ctx,
                            query.after.as_deref(),
                            query.limit.unwrap_or(50),
                            query.include_archived.unwrap_or(false),
                        )?;
                        serde_json::to_value(page).map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_project_get" => {
                let query: McpProjectQuery = serde_json::from_value(args)
                    .map_err(|_| (-32602, "Invalid project lookup arguments"))?;
                state
                    .run(move |store| {
                        let project = store.get_project(&ctx, &query.project_id)?;
                        serde_json::to_value(project).map_err(|_| PlatformError::Storage)
                    })
                    .await
            }
            "nexora_repositories_list"
            | "nexora_repository_get"
            | "nexora_repository_register"
            | "nexora_repository_change" => {
                return RepositoryMcpTools::call(state, ctx, name, args, structured).await;
            }
            "nexora_workflows_list"
            | "nexora_workflow_get"
            | "nexora_workflow_create"
            | "nexora_workflow_change"
            | "nexora_workflow_publish"
            | "nexora_workflow_version_get"
            | "nexora_workflow_versions_list"
            | "nexora_workflow_developers_list"
            | "nexora_developer_workflow_get"
            | "nexora_developer_workflows_list"
            | "nexora_developer_workflow_assign"
            | "nexora_developer_tasks_list" => {
                return crate::workflow_mcp_tools::WorkflowMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            "nexora_organization_documents_list"
            | "nexora_organization_document_get"
            | "nexora_organization_document_write" => {
                return crate::organization_knowledge_mcp::OrganizationKnowledgeMcp::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::work_item_mcp_catalog::WorkItemMcpCatalog::supports(name) => {
                return crate::work_item_mcp_tools::WorkItemMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name
                if crate::task_collaboration_mcp_catalog::TaskCollaborationMcpCatalog::supports(
                    name,
                ) =>
            {
                return crate::task_collaboration_mcp_tools::TaskCollaborationMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::approval_request_mcp_catalog::ApprovalRequestMcpCatalog::supports(
                name,
            ) =>
            {
                return crate::approval_request_mcp_tools::ApprovalRequestMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::document_approval_mcp_catalog::DocumentApprovalMcpCatalog::supports(
                name,
            ) =>
            {
                return crate::document_approval_mcp_tools::DocumentApprovalMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::work_catalog_mcp_catalog::WorkCatalogMcpCatalog::supports(name) => {
                return crate::work_catalog_mcp_tools::WorkCatalogMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::task_lifecycle_mcp_catalog::TaskLifecycleMcpCatalog::supports(name) => {
                return crate::task_lifecycle_mcp_tools::TaskLifecycleMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::traceability_mcp_catalog::TraceabilityMcpCatalog::supports(name) => {
                return crate::traceability_mcp_tools::TraceabilityMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::project_context_mcp_catalog::ProjectContextMcpCatalog::supports(
                name,
            ) =>
            {
                return crate::project_context_mcp_tools::ProjectContextMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::document_mcp_catalog::DocumentMcpCatalog::supports(name) => {
                return crate::document_mcp_tools::DocumentMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::task_template_mcp_catalog::TaskTemplateMcpCatalog::supports(name) => {
                return crate::task_template_mcp_tools::TaskTemplateMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::repository_job_mcp_catalog::RepositoryJobMcpCatalog::supports(name) => {
                return crate::repository_job_mcp_tools::RepositoryJobMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::knowledge_mcp_catalog::KnowledgeMcpCatalog::supports(name) => {
                return crate::knowledge_mcp_tools::KnowledgeMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            name if crate::code_mcp_query::CodeMcpQuery::supports(name) => {
                return crate::code_mcp_tools::CodeMcpTools::call(
                    state, ctx, name, args, structured,
                )
                .await;
            }
            _ => return Err((-32602, "Unknown tool name")),
        };
        McpToolResult::output(result, structured)
    }
}
