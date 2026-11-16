use crate::repository_mcp_catalog::RepositoryMcpCatalog;
use serde_json::{Value, json};
/// Installation-level catalog; every code query resolves its own authorized binding.
pub(crate) struct PlatformMcpCatalog;
impl PlatformMcpCatalog {
    pub fn list() -> Value {
        let mut tools = json!([
            {"name":"nexora_permissions_get","description":"Read current effective permissions for the authenticated actor and this credential, attenuated to API-key project/capability scope. Informational only: every operation rechecks live authorization. Human-only decisions remain unavailable to MCP.","inputSchema":{"type":"object","additionalProperties":false,"properties":{}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}},
            {"name":"nexora_projects_list","description":"List valid projects the current actor can access now. Personal MCP keys automatically follow current membership and role grants, including newly assigned projects; removed access disappears. Follow next_cursor with after to traverse all pages in ID order.","inputSchema":{"type":"object","additionalProperties":false,"properties":{"after":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100},"include_archived":{"type":"boolean"}}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}},
            {"name":"nexora_project_get","description":"Read a project by its logical ID, including archived projects when authorized.","inputSchema":{"type":"object","additionalProperties":false,"required":["project_id"],"properties":{"project_id":{"type":"string"}}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}
        ]);
        tools
            .as_array_mut()
            .unwrap()
            .extend(RepositoryMcpCatalog::list());
        if let Some(items) = tools.as_array_mut() {
            items.extend(crate::organization_knowledge_mcp::OrganizationKnowledgeMcp::list());
            items.extend(crate::knowledge_contribution_mcp::KnowledgeContributionMcp::list());
            items.extend(crate::git_author_mcp_catalog::GitAuthorMcpCatalog::list());
            items.extend(crate::analysis_profile_mcp_catalog::AnalysisProfileMcpCatalog::list());
            items
                .extend(crate::analysis_profile_evolution_mcp::AnalysisProfileEvolutionMcp::list());
            items.extend(crate::feature_mcp_catalog::FeatureMcpCatalog::list());
            items.extend(crate::project_mutation_mcp_catalog::ProjectMutationMcpCatalog::list());
            items.extend(crate::task_attachment_mcp_tools::TaskAttachmentMcpTools::list());
            items.extend(crate::service_request_mcp_tools::ServiceRequestMcpTools::list());
            items.extend(crate::knowledge_review_mcp_tools::KnowledgeReviewMcpTools::list());
            items.extend(crate::task_work_mcp_tools::TaskWorkMcpTools::list());
            items.push(crate::reasoning_mcp_tools::ReasoningMcpTools::catalog());
            items.extend(crate::evidence_mcp_catalog::EvidenceMcpCatalog::list());
            items.extend(crate::code_mcp_catalog::CodeMcpCatalog::list());
            items.extend(crate::knowledge_mcp_catalog::KnowledgeMcpCatalog::list());
            items.extend(crate::repository_job_mcp_catalog::RepositoryJobMcpCatalog::list());
            items.extend(crate::workflow_mcp_catalog::WorkflowMcpCatalog::list());
            items.extend(crate::work_item_mcp_catalog::WorkItemMcpCatalog::list());
            items
                .extend(crate::task_collaboration_mcp_catalog::TaskCollaborationMcpCatalog::list());
            items.extend(crate::task_template_mcp_catalog::TaskTemplateMcpCatalog::list());
            items.extend(crate::document_mcp_catalog::DocumentMcpCatalog::list());
            items.extend(crate::project_context_mcp_catalog::ProjectContextMcpCatalog::list());
            items.extend(crate::traceability_mcp_catalog::TraceabilityMcpCatalog::list());
            items.extend(crate::task_lifecycle_mcp_catalog::TaskLifecycleMcpCatalog::list());
            items.extend(crate::document_approval_mcp_catalog::DocumentApprovalMcpCatalog::list());
            items.extend(crate::work_catalog_mcp_catalog::WorkCatalogMcpCatalog::list());
            items.extend(crate::approval_request_mcp_catalog::ApprovalRequestMcpCatalog::list());
        }
        tools
    }
}
