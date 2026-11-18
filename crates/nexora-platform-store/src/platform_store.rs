use crate::{
    SecretGenerator, authentication_store::AuthenticationStore, credential_store::CredentialStore,
    grant_store::GrantStore, identity_store::IdentityStore, project_store::ProjectStore,
};
use crate::{
    master_key::MasterKey, mutation_request::MutationRequest,
    platform_migrations::PlatformMigrations, repository_store::RepositoryStore,
};
use nexora_platform::*;
use nexora_runtime::DatabaseConnections;
use postgres::{Client, Config, NoTls};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{io::Read, path::PathBuf, time::Duration};

pub struct PlatformStore {
    pub(crate) client: Client,
    pub(crate) namespace: String,
    pub(crate) request_context: Option<MutationRequest>,
    pub(crate) transition_verified_facts: Vec<Value>,
    pub(crate) transition_run_checkpoint: Option<(String, usize)>,
    secret_directory: PathBuf,
    pub(crate) connections: DatabaseConnections,
    pub(crate) code_runtime: Option<nexora_runtime::DatabaseRuntime>,
}
impl PlatformStore {
    pub fn revoke_local_source(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        input: &LocalSourceChange,
    ) -> Result<LocalRepositorySourceInfo, PlatformError> {
        crate::local_source_lifecycle_store::LocalSourceLifecycleStore::revoke(
            self, ctx, project, repository, input,
        )
    }
    pub fn reauthorize_local_source(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &LocalSnapshotAdoption,
    ) -> Result<LocalSnapshotPublication, PlatformError> {
        crate::local_snapshot_adoption_store::LocalSnapshotAdoptionStore::reauthorize(
            self, ctx, project, input,
        )
    }
    pub fn adopt_local_snapshot(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &LocalSnapshotAdoption,
    ) -> Result<LocalSnapshotPublication, PlatformError> {
        crate::local_snapshot_adoption_store::LocalSnapshotAdoptionStore::adopt(
            self, ctx, project, input,
        )
    }
    pub fn create_document_link(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &DocumentLinkCreate,
    ) -> Result<DocumentLink, PlatformError> {
        crate::document_link_store::DocumentLinkStore::create(self, ctx, project, id, input)
    }
    pub fn remove_document_link(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        link: &str,
        input: &DocumentLinkRemove,
    ) -> Result<DocumentLink, PlatformError> {
        crate::document_link_store::DocumentLinkStore::remove(self, ctx, project, id, link, input)
    }
    pub fn document_links_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        after: Option<&str>,
        limit: u16,
        include_removed: bool,
    ) -> Result<DocumentLinkPage, PlatformError> {
        crate::document_link_query_store::DocumentLinkQueryStore::page(
            self,
            ctx,
            project,
            id,
            false,
            after,
            limit,
            include_removed,
        )
    }
    pub fn task_document_links_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        after: Option<&str>,
        limit: u16,
        include_removed: bool,
    ) -> Result<DocumentLinkPage, PlatformError> {
        crate::document_link_query_store::DocumentLinkQueryStore::page(
            self,
            ctx,
            project,
            id,
            true,
            after,
            limit,
            include_removed,
        )
    }
    pub fn create_project_context_package(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &ProjectContextPackageCreate,
    ) -> Result<ProjectContextPackage, PlatformError> {
        crate::context_package_store::ContextPackageStore::create(self, ctx, project, input)
    }
    pub fn get_project_context_package(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ProjectContextPackage, PlatformError> {
        crate::context_package_store::ContextPackageStore::get(self, ctx, project, id)
    }
    pub fn export_project_context_package(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        format: &str,
    ) -> Result<ContextPackageExport, PlatformError> {
        let package = self.get_project_context_package(ctx, project, id)?;
        crate::context_package_renderer::ContextPackageRenderer::render(&package, format)
    }

    pub fn create_project_context_manifest(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &ProjectContextManifestCreate,
    ) -> Result<ProjectContextManifest, PlatformError> {
        crate::project_context_manifest_store::ProjectContextManifestStore::create(
            self, ctx, project, input,
        )
    }
    pub fn get_project_context_manifest(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ProjectContextManifest, PlatformError> {
        crate::project_context_manifest_store::ProjectContextManifestStore::get(
            self, ctx, project, id,
        )
    }
    pub fn project_context_manifest_status(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ProjectContextStatus, PlatformError> {
        crate::project_context_manifest_store::ProjectContextManifestStore::status(
            self, ctx, project, id,
        )
    }

    pub fn project_document_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &DocumentListQuery,
    ) -> Result<DocumentPage, PlatformError> {
        crate::document_catalog_store::DocumentCatalogStore::page(self, ctx, project, input)
    }
    pub fn document_history_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        after: Option<i64>,
        limit: u16,
    ) -> Result<DocumentHistoryPage, PlatformError> {
        crate::document_history_query_store::DocumentHistoryQueryStore::page(
            self, ctx, project, id, after, limit,
        )
    }
    pub fn document_publication_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        after: Option<&str>,
        limit: u16,
    ) -> Result<DocumentPublicationPage, PlatformError> {
        crate::document_publication_query_store::DocumentPublicationQueryStore::page(
            self, ctx, project, id, after, limit,
        )
    }

    pub fn create_project_document(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &ProjectDocumentCreate,
    ) -> Result<ProjectDocument, PlatformError> {
        crate::project_document_definition_store::ProjectDocumentDefinitionStore::create(
            self, ctx, project, input,
        )
    }
    pub fn revise_project_document(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &ProjectDocumentRevise,
    ) -> Result<ProjectDocumentVersion, PlatformError> {
        crate::project_document_definition_store::ProjectDocumentDefinitionStore::revise(
            self, ctx, project, id, input,
        )
    }
    pub fn change_project_document(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &ProjectDocumentChange,
    ) -> Result<ProjectDocument, PlatformError> {
        crate::project_document_definition_store::ProjectDocumentDefinitionStore::change(
            self, ctx, project, id, input,
        )
    }
    pub fn transition_document_version(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        version: &str,
        edge: &str,
        input: &DocumentTransitionRequest,
    ) -> Result<ProjectDocumentVersion, PlatformError> {
        crate::document_transition_store::DocumentTransitionStore::transition(
            self, ctx, project, id, version, edge, input,
        )
    }
    pub fn publish_project_document(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &ProjectDocumentPublish,
    ) -> Result<DocumentPublication, PlatformError> {
        crate::document_publication_store::DocumentPublicationStore::publish(
            self, ctx, project, id, input,
        )
    }
    pub fn get_project_document(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ProjectDocument, PlatformError> {
        crate::document_query_store::DocumentQueryStore::get(self, ctx, project, id)
    }
    pub fn document_version(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        version: &str,
    ) -> Result<ProjectDocumentVersion, PlatformError> {
        crate::document_query_store::DocumentQueryStore::version(self, ctx, project, id, version)
    }
    pub fn document_version_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        before: Option<i64>,
        limit: u16,
    ) -> Result<DocumentVersionPage, PlatformError> {
        crate::document_query_store::DocumentQueryStore::versions(
            self, ctx, project, id, before, limit,
        )
    }
    pub fn export_project_document(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        version: &str,
        format: &str,
    ) -> Result<DocumentExport, PlatformError> {
        let version = self.document_version(ctx, project, id, version)?;
        crate::document_export_renderer::DocumentExportRenderer::render(&version, format)
    }

    pub fn create_task_template(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &TaskTemplateCreate,
    ) -> Result<TaskTemplate, PlatformError> {
        crate::task_template_definition_store::TaskTemplateDefinitionStore::create(
            self, ctx, project, input,
        )
    }
    pub fn change_task_template(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskTemplateChange,
    ) -> Result<TaskTemplate, PlatformError> {
        crate::task_template_definition_store::TaskTemplateDefinitionStore::change(
            self, ctx, project, id, input,
        )
    }
    pub fn publish_task_template_version(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskTemplatePublish,
    ) -> Result<TaskTemplateVersion, PlatformError> {
        crate::task_template_publication_store::TaskTemplatePublicationStore::publish(
            self, ctx, project, id, input,
        )
    }
    pub fn get_task_template(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<TaskTemplate, PlatformError> {
        crate::task_template_query_store::TaskTemplateQueryStore::get(self, ctx, project, id)
    }
    pub fn task_template_version(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        version: &str,
    ) -> Result<TaskTemplateVersion, PlatformError> {
        crate::task_template_query_store::TaskTemplateQueryStore::version(
            self, ctx, project, id, version,
        )
    }
    pub fn task_template_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<TaskTemplatePage, PlatformError> {
        crate::task_template_query_store::TaskTemplateQueryStore::page(
            self,
            ctx,
            project,
            after,
            limit,
            include_archived,
        )
    }
    pub fn task_template_version_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        before: Option<i64>,
        limit: u16,
    ) -> Result<TaskTemplateVersionPage, PlatformError> {
        crate::task_template_query_store::TaskTemplateQueryStore::versions(
            self, ctx, project, id, before, limit,
        )
    }
    pub fn apply_task_template(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &TaskTemplateApply,
    ) -> Result<WorkTask, PlatformError> {
        crate::task_template_application_store::TaskTemplateApplicationStore::apply(
            self, ctx, project, input,
        )
    }

    pub fn create_task_dependency(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &TaskDependencyCreate,
    ) -> Result<TaskDependency, PlatformError> {
        crate::task_dependency_creation_store::TaskDependencyCreationStore::create(
            self, ctx, project, input,
        )
    }
    pub fn resolve_task_dependency(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskDependencyChange,
    ) -> Result<TaskDependency, PlatformError> {
        crate::task_dependency_lifecycle_store::TaskDependencyLifecycleStore::resolve(
            self, ctx, project, id, input,
        )
    }
    pub fn remove_task_dependency(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskDependencyChange,
    ) -> Result<TaskDependency, PlatformError> {
        crate::task_dependency_lifecycle_store::TaskDependencyLifecycleStore::remove(
            self, ctx, project, id, input,
        )
    }
    pub fn task_dependency_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        task: &str,
        input: &TaskDependencyQuery,
    ) -> Result<TaskDependencyPage, PlatformError> {
        crate::task_dependency_query_store::TaskDependencyQueryStore::page(
            self,
            ctx,
            Some(project),
            task,
            input,
        )
    }
    pub fn add_task_comment(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        task: &str,
        input: &TaskCommentCreate,
    ) -> Result<TaskComment, PlatformError> {
        crate::task_comment_store::TaskCommentStore::add(self, ctx, Some(project), task, input)
    }
    pub fn task_comment_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        task: &str,
        after: Option<&str>,
        limit: u16,
    ) -> Result<TaskCommentPage, PlatformError> {
        crate::task_comment_query_store::TaskCommentQueryStore::page(
            self,
            ctx,
            Some(project),
            task,
            after,
            limit,
        )
    }

    pub fn create_approval_request(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &ApprovalRequestCreate,
    ) -> Result<ApprovalRequest, PlatformError> {
        crate::approval_request_definition_store::ApprovalRequestDefinitionStore::create(
            self, ctx, project, input,
        )
    }
    pub fn decide_approval_request(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &ApprovalRequestDecide,
    ) -> Result<ApprovalRequest, PlatformError> {
        crate::approval_request_decision_store::ApprovalRequestDecisionStore::decide(
            self, ctx, project, id, input,
        )
    }
    pub fn cancel_approval_request(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &ApprovalRequestCancel,
    ) -> Result<ApprovalRequest, PlatformError> {
        crate::approval_request_decision_store::ApprovalRequestDecisionStore::cancel(
            self, ctx, project, id, input,
        )
    }
    pub fn get_approval_request(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ApprovalRequest, PlatformError> {
        crate::approval_request_query_store::ApprovalRequestQueryStore::get(self, ctx, project, id)
    }
    pub fn approval_request_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        after: Option<&str>,
        limit: u16,
        status: Option<ApprovalRequestStatus>,
    ) -> Result<ApprovalRequestPage, PlatformError> {
        crate::approval_request_query_store::ApprovalRequestQueryStore::page(
            self, ctx, project, after, limit, status,
        )
    }
    pub fn get_document_approval(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        document: &str,
        version: &str,
        id: &str,
    ) -> Result<DocumentApproval, PlatformError> {
        crate::document_approval_query_store::DocumentApprovalQueryStore::get(
            self, ctx, project, document, version, id,
        )
    }
    pub fn revoke_document_approval(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        document: &str,
        version: &str,
        id: &str,
        input: &DocumentApprovalRevoke,
    ) -> Result<DocumentApproval, PlatformError> {
        crate::document_approval_revocation_store::DocumentApprovalRevocationStore::revoke(
            self, ctx, project, document, version, id, input,
        )
    }
    pub fn task_type_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<TaskTypePage, PlatformError> {
        crate::task_type_query_store::TaskTypeQueryStore::page(
            self,
            ctx,
            project,
            after,
            limit,
            include_archived,
        )
    }
    pub fn backlog_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<BacklogPage, PlatformError> {
        crate::backlog_query_store::BacklogQueryStore::page(
            self,
            ctx,
            project,
            after,
            limit,
            include_archived,
        )
    }
    pub fn change_task_type(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        code: &str,
        input: &TaskTypeChange,
    ) -> Result<TaskType, PlatformError> {
        crate::task_type_change_store::TaskTypeChangeStore::change(self, ctx, project, code, input)
    }
    pub fn get_task_approval(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        task: &str,
        id: &str,
    ) -> Result<TaskApproval, PlatformError> {
        crate::task_approval_query_store::TaskApprovalQueryStore::get(
            self,
            ctx,
            Some(project),
            task,
            id,
        )
    }
    pub fn revoke_task_approval(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        task: &str,
        id: &str,
        input: &TaskApprovalRevoke,
    ) -> Result<TaskApproval, PlatformError> {
        crate::task_approval_revocation_store::TaskApprovalRevocationStore::revoke(
            self,
            ctx,
            Some(project),
            task,
            id,
            input,
        )
    }

    pub fn grant_task_approval(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskApprovalGrant,
    ) -> Result<TaskApproval, PlatformError> {
        crate::task_approval_store::TaskApprovalStore::grant(self, ctx, Some(project), id, input)
    }
    pub fn create_task_type(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &TaskTypeCreate,
    ) -> Result<TaskType, PlatformError> {
        crate::task_type_store::TaskTypeStore::create(self, ctx, project, input)
    }
    pub fn create_backlog(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &BacklogCreate,
    ) -> Result<Backlog, PlatformError> {
        crate::backlog_store::BacklogStore::create(self, ctx, project, input)
    }
    pub fn change_backlog(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &BacklogChange,
    ) -> Result<Backlog, PlatformError> {
        crate::backlog_store::BacklogStore::change(self, ctx, project, id, input)
    }
    pub fn get_backlog(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<Backlog, PlatformError> {
        crate::backlog_store::BacklogStore::get(self, ctx, project, id)
    }
    pub fn reorder_backlog(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &BacklogReorder,
    ) -> Result<Backlog, PlatformError> {
        crate::backlog_order_store::BacklogOrderStore::reorder(self, ctx, project, id, input)
    }
    /// Legacy project-policy compatibility; REST and MCP create tasks through create_developer_task.
    pub fn create_task(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &TaskCreate,
    ) -> Result<WorkTask, PlatformError> {
        crate::task_definition_store::TaskDefinitionStore::create(self, ctx, project, input, false)
    }
    pub fn change_task(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskChange,
    ) -> Result<WorkTask, PlatformError> {
        crate::task_definition_store::TaskDefinitionStore::change(
            self,
            ctx,
            Some(project),
            id,
            input,
        )
    }
    pub fn move_task(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskMove,
    ) -> Result<WorkTask, PlatformError> {
        crate::task_movement_store::TaskMovementStore::move_task(self, ctx, project, id, input)
    }
    pub fn migrate_task_workflow(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskWorkflowMigration,
    ) -> Result<WorkTask, PlatformError> {
        crate::task_workflow_migration_store::TaskWorkflowMigrationStore::migrate(
            self, ctx, project, id, input,
        )
    }
    pub fn transition_task(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        transition: &str,
        input: &TaskTransitionRequest,
    ) -> Result<WorkTask, PlatformError> {
        crate::task_transition_store::TaskTransitionStore::transition(
            self,
            ctx,
            Some(project),
            id,
            transition,
            input,
        )
    }
    pub fn get_task(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<WorkTask, PlatformError> {
        crate::task_query_store::TaskQueryStore::get(self, ctx, Some(project), id)
    }
    pub fn task_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &TaskListQuery,
    ) -> Result<TaskPage, PlatformError> {
        crate::task_query_store::TaskQueryStore::page(self, ctx, project, input)
    }
    pub fn task_history(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        after: Option<i64>,
        limit: u16,
    ) -> Result<TaskHistoryPage, PlatformError> {
        crate::task_query_store::TaskQueryStore::history(self, ctx, Some(project), id, after, limit)
    }

    pub fn workflow_version_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        before: Option<i64>,
        limit: u16,
    ) -> Result<WorkflowVersionPage, PlatformError> {
        crate::workflow_query_store::WorkflowQueryStore::versions(
            self,
            ctx,
            Some(project),
            id,
            before,
            limit,
        )
    }
    pub fn create_workflow(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &WorkflowCreate,
    ) -> Result<WorkflowDefinition, PlatformError> {
        crate::workflow_definition_store::WorkflowDefinitionStore::create(
            self,
            ctx,
            Some(project),
            input,
        )
    }
    pub fn change_workflow(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &WorkflowChange,
    ) -> Result<WorkflowDefinition, PlatformError> {
        crate::workflow_definition_store::WorkflowDefinitionStore::change(
            self,
            ctx,
            Some(project),
            id,
            input,
        )
    }
    pub fn publish_workflow(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &WorkflowPublish,
    ) -> Result<WorkflowVersion, PlatformError> {
        crate::workflow_publication_store::WorkflowPublicationStore::publish(
            self,
            ctx,
            Some(project),
            id,
            input,
        )
    }
    pub fn get_workflow(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<WorkflowDefinition, PlatformError> {
        crate::workflow_query_store::WorkflowQueryStore::get(self, ctx, Some(project), id)
    }
    pub fn workflow_version(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        version: &str,
    ) -> Result<WorkflowVersion, PlatformError> {
        crate::workflow_query_store::WorkflowQueryStore::version(
            self,
            ctx,
            Some(project),
            id,
            version,
        )
    }
    pub fn workflow_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<WorkflowPage, PlatformError> {
        crate::workflow_query_store::WorkflowQueryStore::page(
            self,
            ctx,
            Some(project),
            after,
            limit,
            include_archived,
        )
    }
    pub fn query_repository_knowledge(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        snapshot: Option<&str>,
        command: &nexora_runtime::KnowledgeCommand,
    ) -> Result<Value, PlatformError> {
        crate::repository_knowledge_store::RepositoryKnowledgeStore::query(
            self, ctx, project, repository, snapshot, command,
        )
    }
    pub fn pending_repository_knowledge(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        snapshot: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Value, PlatformError> {
        crate::repository_knowledge_store::RepositoryKnowledgeStore::pending(
            self, ctx, project, repository, snapshot, offset, limit,
        )
    }
    pub fn query_repository_code(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        snapshot: Option<&str>,
        query: &nexora_runtime::KnowledgeQuery,
    ) -> Result<Value, PlatformError> {
        crate::repository_code_store::RepositoryCodeStore::query(
            self, ctx, project, repository, snapshot, query,
        )
    }
    pub fn repository_binding(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<RepositoryBinding, PlatformError> {
        crate::repository_binding_store::RepositoryBindingStore::get(self, ctx, project, repository)
    }
    pub fn fail_repository_job(
        &mut self,
        lease: &crate::RepositoryJobLease,
        failure: RepositoryJobFailure,
    ) -> Result<RepositoryJob, PlatformError> {
        crate::repository_job_failure_store::RepositoryJobFailureStore::record(self, lease, failure)
    }
    pub fn prepare_repository_job_source(
        &mut self,
        lease: &crate::RepositoryJobLease,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<crate::RepositoryJobSourcePlan, PlatformError> {
        crate::repository_job_source_plan_store::RepositoryJobSourcePlanStore::new(self)
            .prepare(lease, policy)
    }
    pub fn pin_repository_job(
        &mut self,
        lease: &crate::RepositoryJobLease,
        commit: &str,
    ) -> Result<crate::RepositoryRuntimeScope, PlatformError> {
        crate::repository_job_publication_store::RepositoryJobPublicationStore::new(self)
            .pin(lease, commit)
    }
    pub fn complete_repository_job(
        &mut self,
        lease: &crate::RepositoryJobLease,
        snapshot: &nexora_runtime::QuerySnapshot,
    ) -> Result<RepositoryBinding, PlatformError> {
        crate::repository_job_publication_store::RepositoryJobPublicationStore::new(self)
            .complete(lease, snapshot)
    }
    pub fn complete_repository_job_with_profile_selection(
        &mut self,
        lease: &crate::RepositoryJobLease,
        snapshot: &nexora_runtime::QuerySnapshot,
        selection: &serde_json::Value,
    ) -> Result<RepositoryBinding, PlatformError> {
        crate::repository_job_publication_store::RepositoryJobPublicationStore::new(self)
            .complete_with_selection(lease, snapshot, Some(selection))
    }
    pub fn claim_repository_job(
        &mut self,
        worker: &str,
        seconds: u16,
    ) -> Result<Option<crate::RepositoryJobLease>, PlatformError> {
        crate::repository_job_lease_store::RepositoryJobLeaseStore::new(self).claim(worker, seconds)
    }
    pub fn renew_repository_job(
        &mut self,
        lease: &crate::RepositoryJobLease,
        seconds: u16,
    ) -> Result<crate::RepositoryJobLease, PlatformError> {
        crate::repository_job_lease_store::RepositoryJobLeaseStore::new(self).renew(lease, seconds)
    }
    pub fn pending_repository_jobs_for_wakeup(
        &mut self,
    ) -> Result<Vec<RepositoryJob>, PlatformError> {
        crate::repository_job_pending_wakeups::RepositoryJobPendingWakeups::list(self)
    }
    pub fn pending_job_wakeups(
        &mut self,
    ) -> Result<Vec<nexora_platform::JobWakeup>, PlatformError> {
        crate::pending_job_wakeups::PendingJobWakeups::list(self)
    }
    pub fn enqueue_repository_job(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        kind: RepositoryJobKind,
        input: &RepositoryJobRequest,
    ) -> Result<RepositoryJob, PlatformError> {
        crate::repository_job_store::RepositoryJobStore::new(self)
            .enqueue(ctx, project, repository, kind, input)
    }
    pub fn repository_job(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<RepositoryJob, PlatformError> {
        crate::repository_job_store::RepositoryJobStore::new(self).get(ctx, project, id)
    }
    pub fn cancel_repository_job(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &RepositoryJobChange,
    ) -> Result<RepositoryJob, PlatformError> {
        crate::repository_job_store::RepositoryJobStore::new(self).cancel(ctx, project, id, input)
    }
    pub fn connect(c: &DatabaseConnections, namespace: &str) -> Result<Self, PlatformError> {
        c.validate()
            .map_err(|_| PlatformError::Validation("invalid database configuration"))?;
        if namespace.is_empty()
            || namespace.len() > 63
            || !namespace.starts_with("nexora_platform")
            || !namespace
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(PlatformError::Validation("invalid platform namespace"));
        }
        let secret_path = c.secrets_directory.join("postgres_password");
        let metadata =
            std::fs::symlink_metadata(&secret_path).map_err(|_| PlatformError::Storage)?;
        if !metadata.file_type().is_file() {
            return Err(PlatformError::Storage);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(PlatformError::Validation(
                    "database secret file permissions are too broad",
                ));
            }
        }
        let mut password = String::new();
        std::fs::File::open(secret_path)
            .map_err(|_| PlatformError::Storage)?
            .take(4097)
            .read_to_string(&mut password)
            .map_err(|_| PlatformError::Storage)?;
        let password = password.trim();
        if password.is_empty() || password.len() > 1024 || password.contains(['\r', '\n']) {
            return Err(PlatformError::Storage);
        }
        let client=Config::new().host(&c.host).port(c.postgres_port).user(&c.postgres_user).password(password).dbname(&c.postgres_database).connect_timeout(Duration::from_secs(5)).options(&format!("-c search_path={namespace},nexora_platform,pg_catalog -c timezone=UTC -c statement_timeout=15000 -c lock_timeout=10000")).connect(NoTls).map_err(Self::database_error)?;
        Ok(Self {
            client,
            namespace: namespace.into(),
            request_context: None,
            transition_run_checkpoint: None,
            transition_verified_facts: Vec::new(),
            secret_directory: c.secrets_directory.clone(),
            connections: c.clone(),
            code_runtime: None,
        })
    }
    pub fn initialize(&mut self) -> Result<(), PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("nexora-platform-schema:{}", self.namespace)],
        )
        .map_err(Self::database_error)?;
        tx.batch_execute(
            "CREATE SCHEMA IF NOT EXISTS nexora_platform; CREATE EXTENSION IF NOT EXISTS vector SCHEMA nexora_platform;",
        )
        .map_err(Self::database_error)?;
        tx.batch_execute(&format!(
            "CREATE SCHEMA IF NOT EXISTS \"{}\"",
            self.namespace
        ))
        .map_err(Self::database_error)?;
        tx.batch_execute(include_str!("../migrations/0001_platform.sql"))
            .map_err(Self::database_error)?;
        let versions = tx
            .query(
                "SELECT version,checksum FROM platform_schema_versions ORDER BY version",
                &[],
            )
            .map_err(Self::database_error)?;
        for row in &versions {
            let version = row.get::<_, i32>(0);
            let migration = PlatformMigrations::all()
                .iter()
                .find(|(number, _)| *number == version)
                .ok_or(PlatformError::Conflict("unsupported platform migration"))?
                .1;
            let expected = SecretGenerator::checksum(migration.as_bytes());
            if row.get::<_, String>(1) != expected {
                return Err(PlatformError::Conflict(
                    "platform migration checksum mismatch",
                ));
            }
        }
        for &(version, migration) in PlatformMigrations::all() {
            if !versions.iter().any(|r| r.get::<_, i32>(0) == version) {
                tx.batch_execute(migration).map_err(Self::database_error)?;
                tx.execute(
                    "INSERT INTO platform_schema_versions(version,checksum) VALUES($1,$2)",
                    &[&version, &SecretGenerator::checksum(migration.as_bytes())],
                )
                .map_err(Self::database_error)?;
            }
        }
        let path = MasterKey::path(&self.secret_directory, &self.namespace);
        let existing = tx
            .query_opt(
                "SELECT key_id FROM platform_crypto_state WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?;
        let key = if existing.is_some() {
            MasterKey::load(&path)?
        } else {
            MasterKey::create_or_load(&path)?
        };
        if existing.is_some_and(|row| row.get::<_, String>(0) != key.id()) {
            return Err(PlatformError::Conflict(
                "installation master key does not match database",
            ));
        }
        tx.execute("INSERT INTO platform_crypto_state(key_version,key_id) VALUES(1,$1) ON CONFLICT DO NOTHING",&[&key.id()]).map_err(Self::database_error)?;
        tx.commit().map_err(Self::database_error)
    }
    pub fn ready(&mut self) -> Result<bool, PlatformError> {
        let versions = match self.client.query(
            "SELECT version,checksum FROM platform_schema_versions ORDER BY version",
            &[],
        ) {
            Ok(rows) => rows,
            Err(e) if e.code() == Some(&postgres::error::SqlState::UNDEFINED_TABLE) => {
                return Ok(false);
            }
            Err(e) => return Err(Self::database_error(e)),
        };
        if versions.len() != PlatformMigrations::all().len() {
            return Ok(false);
        }
        for (row, &(version, sql)) in versions.iter().zip(PlatformMigrations::all()) {
            if row.get::<_, i32>(0) != version
                || row.get::<_, String>(1) != SecretGenerator::checksum(sql.as_bytes())
            {
                return Ok(false);
            }
        }
        let key = MasterKey::load(&MasterKey::path(&self.secret_directory, &self.namespace))?;
        let key_id = self
            .client
            .query_opt(
                "SELECT key_id FROM platform_crypto_state WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?;
        if key_id.is_none_or(|r| r.get::<_, String>(0) != key.id()) {
            return Ok(false);
        }
        self.client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM installation WHERE singleton)",
                &[],
            )
            .map(|r| r.get(0))
            .map_err(Self::database_error)
    }
    pub fn request<T, F>(
        &mut self,
        ctx: &AuthContext,
        operation: &str,
        request_id: &str,
        input: &Value,
        action: F,
    ) -> Result<T, PlatformError>
    where
        F: FnOnce(&mut Self) -> Result<T, PlatformError>,
    {
        if self.request_context.is_some() {
            return Err(PlatformError::Conflict("nested mutation request"));
        }
        if request_id.is_empty()
            || request_id.len() > 128
            || !request_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
            || request_id.starts_with("nxk_")
            || request_id.starts_with("nxs_")
        {
            return Err(PlatformError::Validation("invalid idempotency key"));
        }
        if operation.is_empty() || operation.len() > 128 {
            return Err(PlatformError::Validation("invalid mutation operation"));
        }
        let current = self.fresh(ctx)?;
        let key = self.installation_key()?;
        let bytes = serde_json::to_vec(&(
            &self.namespace,
            current.actor_id(),
            operation,
            request_id,
            input,
        ))
        .map_err(|_| PlatformError::Storage)?;
        if bytes.len() > 1_048_576 {
            return Err(PlatformError::Validation("request exceeds limit"));
        }
        let fingerprint = key.fingerprint(&bytes)?;
        self.request_context = Some(MutationRequest {
            actor_id: current.actor_id().into(),
            operation: operation.into(),
            request_id: request_id.into(),
            fingerprint,
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| action(self)));
        self.request_context = None;
        match result {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    fn installation_key(&mut self) -> Result<MasterKey, PlatformError> {
        let key = MasterKey::load(&MasterKey::path(&self.secret_directory, &self.namespace))?;
        let state = self
            .client
            .query_one(
                "SELECT key_version,key_id FROM platform_crypto_state WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?;
        if state.get::<_, i32>(0) != 1 || state.get::<_, String>(1) != key.id() {
            return Err(PlatformError::Conflict("installation master key mismatch"));
        }
        Ok(key)
    }
    /// Credential adapters use this verified installation key; no HTTP route exposes this primitive.
    pub(crate) fn email_token_cipher(
        &mut self,
    ) -> Result<crate::email_token_cipher::EmailTokenCipher, PlatformError> {
        crate::email_token_cipher::EmailTokenCipher::new(self.installation_key()?, &self.namespace)
    }
    pub fn provider_secret_cipher(&mut self) -> Result<crate::ProviderSecretCipher, PlatformError> {
        let key = self.installation_key()?;
        Ok(crate::ProviderSecretCipher::new(key, &self.namespace))
    }
    pub(crate) fn database_error(error: postgres::Error) -> PlatformError {
        match error.code() {
            Some(c) if c == &postgres::error::SqlState::UNIQUE_VIOLATION => {
                PlatformError::Conflict("resource already exists")
            }
            Some(c)
                if c == &postgres::error::SqlState::FOREIGN_KEY_VIOLATION
                    || c == &postgres::error::SqlState::CHECK_VIOLATION =>
            {
                PlatformError::Validation("invalid resource relation")
            }
            _ => PlatformError::Storage,
        }
    }
    pub(crate) fn decode<T: DeserializeOwned>(value: Value) -> Result<T, PlatformError> {
        serde_json::from_value(value).map_err(|_| PlatformError::Storage)
    }
    pub fn bootstrap(
        &mut self,
        organization: &str,
        username: &str,
        password: &str,
        name: &str,
    ) -> Result<BootstrapResult, PlatformError> {
        IdentityStore::new(self).bootstrap(organization, username, password, name)
    }
    pub fn login(
        &mut self,
        username: &str,
        password: &str,
    ) -> Result<IssuedSession, PlatformError> {
        IdentityStore::new(self).login(username, password)
    }
    pub fn login_from_peer(
        &mut self,
        username: &str,
        password: &str,
        peer: Option<&str>,
    ) -> Result<IssuedSession, PlatformError> {
        IdentityStore::new(self).login_from_peer(username, password, peer)
    }
    pub fn consume_authentication_attempt(
        &mut self,
        key: &str,
        limit: i64,
        window_seconds: i64,
    ) -> Result<(), PlatformError> {
        IdentityStore::new(self).consume_authentication_attempt(key, limit, window_seconds)
    }
    pub fn logout(&mut self, token: &str) -> Result<(), PlatformError> {
        IdentityStore::new(self).logout(token)
    }
    pub fn authenticate(&mut self, token: &str) -> Result<AuthContext, PlatformError> {
        AuthenticationStore::new(self).authenticate(token)
    }
    pub(crate) fn fresh(&mut self, ctx: &AuthContext) -> Result<AuthContext, PlatformError> {
        AuthenticationStore::new(self).refresh(ctx)
    }
    pub fn create_actor(
        &mut self,
        ctx: &AuthContext,
        kind: ActorKind,
        name: &str,
        purpose: &str,
    ) -> Result<Actor, PlatformError> {
        IdentityStore::new(self).create_actor(ctx, kind, name, purpose)
    }
    pub fn create_user(
        &mut self,
        ctx: &AuthContext,
        username: &str,
        password: &str,
        name: &str,
    ) -> Result<Actor, PlatformError> {
        IdentityStore::new(self).create_user(ctx, username, password, name)
    }
    pub fn suspend_actor(&mut self, ctx: &AuthContext, id: &str) -> Result<(), PlatformError> {
        IdentityStore::new(self).suspend(ctx, id)
    }
    pub fn grant(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        capability: Capability,
        project: Option<&str>,
    ) -> Result<(), PlatformError> {
        GrantStore::new(self).grant(ctx, actor, capability, project)
    }
    pub fn revoke_grant(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        capability: Capability,
        project: Option<&str>,
    ) -> Result<(), PlatformError> {
        GrantStore::new(self).revoke(ctx, actor, capability, project)
    }
    pub fn create_project(
        &mut self,
        ctx: &AuthContext,
        name: &str,
        description: Option<&str>,
    ) -> Result<Project, PlatformError> {
        self.create_project_with_color(ctx, name, description, None)
    }
    pub fn create_project_with_color(
        &mut self,
        ctx: &AuthContext,
        name: &str,
        description: Option<&str>,
        color: Option<&HexColor>,
    ) -> Result<Project, PlatformError> {
        ProjectStore::new(self).create(ctx, name, description, color)
    }
    pub fn list_projects(&mut self, ctx: &AuthContext) -> Result<Vec<Project>, PlatformError> {
        ProjectStore::new(self).list(ctx)
    }
    pub fn get_project(&mut self, ctx: &AuthContext, id: &str) -> Result<Project, PlatformError> {
        ProjectStore::new(self).get(ctx, id)
    }
    pub fn change_project(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        change: &ProjectChange,
    ) -> Result<Project, PlatformError> {
        ProjectStore::new(self).change(ctx, id, change)
    }
    pub fn project_page(
        &mut self,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<ProjectPage, PlatformError> {
        ProjectStore::new(self).page(ctx, after, limit, include_archived)
    }
    pub fn authentication_options(&mut self) -> Result<Value, PlatformError> {
        let policy = self.authentication_policy()?;
        let providers:Vec<Value>=self.client.query("SELECT jsonb_build_object('id',id,'name',name,'issuer',issuer) FROM oidc_providers WHERE enabled ORDER BY id LIMIT 100",&[]).map_err(Self::database_error)?.iter().map(|r|r.get(0)).collect();
        Ok(
            serde_json::json!({"local_enabled":policy.local_enabled,"revision":policy.revision,"oidc":providers}),
        )
    }
    pub fn change_oidc_provider(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        change: &OidcProviderChange,
        policy: &crate::OidcNetworkPolicy,
    ) -> Result<OidcProvider, PlatformError> {
        let policy = self.installation_oidc_policy(policy)?;
        crate::oidc_settings_store::OidcSettingsStore::new(self).change(ctx, id, change, &policy)
    }
    pub fn configure_oidc(
        &mut self,
        ctx: &AuthContext,
        input: &OidcProviderRegistration,
        policy: &crate::OidcNetworkPolicy,
    ) -> Result<OidcProvider, PlatformError> {
        let policy = self.installation_oidc_policy(policy)?;
        crate::oidc_settings_store::OidcSettingsStore::new(self).configure(ctx, input, &policy)
    }
    pub fn oidc_providers(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<Vec<OidcProvider>, PlatformError> {
        crate::oidc_settings_store::OidcSettingsStore::new(self).list(ctx)
    }
    pub fn authentication_policy(&mut self) -> Result<AuthenticationPolicy, PlatformError> {
        crate::oidc_settings_store::OidcSettingsStore::new(self).authentication_policy()
    }
    pub fn change_authentication_policy(
        &mut self,
        ctx: &AuthContext,
        input: &AuthenticationPolicyChange,
    ) -> Result<AuthenticationPolicy, PlatformError> {
        crate::oidc_settings_store::OidcSettingsStore::new(self).change_policy(ctx, input)
    }
    pub fn create_oidc_user(
        &mut self,
        ctx: &AuthContext,
        input: &OidcUserRegistration,
    ) -> Result<Actor, PlatformError> {
        crate::oidc_account_store::OidcAccountStore::new(self).create_user(ctx, input)
    }
    pub fn assign_oidc_identity(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        input: &OidcIdentityAssignment,
    ) -> Result<(), PlatformError> {
        crate::oidc_account_store::OidcAccountStore::new(self).assign(ctx, actor, input)
    }
    pub fn start_oidc(
        &mut self,
        provider: &str,
        policy: &crate::OidcNetworkPolicy,
    ) -> Result<OidcAuthorization, PlatformError> {
        let policy = self.installation_oidc_policy(policy)?;
        crate::oidc_authorization_store::OidcAuthorizationStore::new(self).start(provider, &policy)
    }
    pub fn complete_oidc(
        &mut self,
        state: &str,
        code: &str,
        cookie: &str,
        policy: &crate::OidcNetworkPolicy,
    ) -> Result<IssuedSession, PlatformError> {
        let policy = self.installation_oidc_policy(policy)?;
        crate::oidc_authorization_store::OidcAuthorizationStore::new(self)
            .complete(state, code, cookie, &policy)
    }
    pub fn receive_github_webhook(
        &mut self,
        proof: &crate::WebhookProof,
        body: &[u8],
        policy: &crate::GitNetworkPolicy,
    ) -> Result<WebhookReceipt, PlatformError> {
        crate::github_webhook_store::GithubWebhookStore::new(self).receive(proof, body, policy)
    }
    pub fn register_gitlab_webhook(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        input: &GitlabWebhookRegistration,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GitlabWebhook, PlatformError> {
        crate::gitlab_webhook_store::GitlabWebhookStore::new(self)
            .register(ctx, project, repository, input, policy)
    }
    pub fn gitlab_webhooks(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<Vec<GitlabWebhook>, PlatformError> {
        crate::gitlab_webhook_store::GitlabWebhookStore::new(self).list(ctx, project, repository)
    }
    pub fn revoke_gitlab_webhook(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        id: &str,
    ) -> Result<(), PlatformError> {
        crate::gitlab_webhook_store::GitlabWebhookStore::new(self)
            .revoke(ctx, project, repository, id)
    }
    pub fn receive_gitlab_webhook(
        &mut self,
        id: &str,
        proof: &crate::WebhookProof,
        body: &[u8],
        policy: &crate::GitNetworkPolicy,
    ) -> Result<WebhookReceipt, PlatformError> {
        crate::gitlab_webhook_store::GitlabWebhookStore::new(self).receive(id, proof, body, policy)
    }
    pub fn start_github_manifest(
        &mut self,
        ctx: &AuthContext,
        input: &GithubManifestRequest,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GithubManifestAuthorization, PlatformError> {
        crate::github_manifest_store::GithubManifestStore::new(self).start(ctx, input, policy)
    }
    pub fn complete_github_manifest(
        &mut self,
        state: &str,
        code: &str,
        cookie: &str,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GithubConfiguration, PlatformError> {
        crate::github_manifest_store::GithubManifestStore::new(self)
            .complete(state, code, cookie, policy)
    }
    pub fn github_configuration(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<GithubConfiguration, PlatformError> {
        crate::github_settings_store::GithubSettingsStore::new(self).metadata(ctx)
    }
    pub fn gitlab_configuration(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<GitlabConfiguration, PlatformError> {
        crate::gitlab_settings_store::GitlabSettingsStore::new(self).metadata(ctx)
    }
    pub fn change_github_configuration_state(
        &mut self,
        ctx: &AuthContext,
        input: &GitConfigurationStateChange,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GithubConfiguration, PlatformError> {
        let value = crate::git_configuration_state_store::GitConfigurationStateStore::new(self)
            .change(ctx, GitProvider::Github, input, policy)?;
        Self::decode(value)
    }
    pub fn change_gitlab_configuration_state(
        &mut self,
        ctx: &AuthContext,
        input: &GitConfigurationStateChange,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GitlabConfiguration, PlatformError> {
        let value = crate::git_configuration_state_store::GitConfigurationStateStore::new(self)
            .change(ctx, GitProvider::Gitlab, input, policy)?;
        Self::decode(value)
    }
    pub fn configure_github(
        &mut self,
        ctx: &AuthContext,
        input: &GithubAppRegistration,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GithubConfiguration, PlatformError> {
        crate::github_settings_store::GithubSettingsStore::new(self).configure(ctx, input, policy)
    }
    pub fn start_github(
        &mut self,
        ctx: &AuthContext,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GitAuthorization, PlatformError> {
        crate::github_authorization_store::GithubAuthorizationStore::new(self).start(ctx, policy)
    }
    pub fn complete_github(
        &mut self,
        state: &str,
        code: &str,
        cookie: &str,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GithubConsent, PlatformError> {
        crate::github_authorization_store::GithubAuthorizationStore::new(self)
            .complete(state, code, cookie, policy)
    }
    pub fn github_installations(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        page: u32,
        limit: u16,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GithubInstallationPage, PlatformError> {
        crate::github_consent_store::GithubConsentStore::new(self)
            .installations(ctx, id, page, limit, policy)
    }
    pub fn select_github_installation(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &GithubInstallationSelection,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GitConnection, PlatformError> {
        crate::github_consent_store::GithubConsentStore::new(self).select(ctx, id, input, policy)
    }
    pub fn configure_gitlab(
        &mut self,
        ctx: &AuthContext,
        input: &GitlabClientRegistration,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GitlabConfiguration, PlatformError> {
        crate::gitlab_settings_store::GitlabSettingsStore::new(self).configure(ctx, input, policy)
    }
    pub fn git_connections(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<Vec<GitConnection>, PlatformError> {
        crate::git_connection_store::GitConnectionStore::new(self).list(ctx)
    }
    pub fn git_connection_page(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
        after: Option<&str>,
        limit: u16,
    ) -> Result<GitConnectionPage, PlatformError> {
        crate::git_connection_store::GitConnectionStore::new(self).page(ctx, project, after, limit)
    }
    pub fn connection_remote_repositories(
        &mut self,
        ctx: &AuthContext,
        connection: &str,
        page: u32,
        limit: u16,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<RemoteRepositoryPage, PlatformError> {
        crate::remote_repository_store::RemoteRepositoryStore::new(self)
            .list_available(ctx, connection, page, limit, policy)
    }
    pub fn remote_repositories(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        connection: &str,
        page: u32,
        limit: u16,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<RemoteRepositoryPage, PlatformError> {
        crate::remote_repository_store::RemoteRepositoryStore::new(self)
            .list(ctx, project, connection, page, limit, policy)
    }
    pub fn start_gitlab(
        &mut self,
        ctx: &AuthContext,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GitAuthorization, PlatformError> {
        crate::gitlab_authorization_store::GitlabAuthorizationStore::new(self).start(ctx, policy)
    }
    pub fn complete_gitlab(
        &mut self,
        state: &str,
        code: &str,
        cookie: &str,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<GitConnection, PlatformError> {
        crate::gitlab_authorization_store::GitlabAuthorizationStore::new(self)
            .complete(state, code, cookie, policy)
    }
    pub fn revoke_git_connection(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<(), PlatformError> {
        crate::git_connection_store::GitConnectionStore::new(self).revoke(ctx, id)
    }
    pub fn share_git_connection(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        project: &str,
    ) -> Result<(), PlatformError> {
        crate::git_connection_store::GitConnectionStore::new(self).share(ctx, id, project)
    }
    pub fn unshare_git_connection(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        project: &str,
    ) -> Result<(), PlatformError> {
        crate::git_connection_store::GitConnectionStore::new(self).unshare(ctx, id, project)
    }
    pub fn authorize_repository(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        input: &GitRepositoryAuthorization,
        policy: &crate::GitNetworkPolicy,
    ) -> Result<ProjectRepository, PlatformError> {
        crate::git_repository_store::GitRepositoryStore::new(self)
            .authorize(ctx, project, repository, input, policy)
    }
    pub fn register_repository(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &RepositoryRegistration,
    ) -> Result<ProjectRepository, PlatformError> {
        RepositoryStore::new(self).register(ctx, project, input)
    }
    pub fn get_repository(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ProjectRepository, PlatformError> {
        RepositoryStore::new(self).get(ctx, project, id)
    }
    pub fn repository_page(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        after: Option<&str>,
        limit: u16,
        include_archived: bool,
    ) -> Result<RepositoryPage, PlatformError> {
        RepositoryStore::new(self).page(ctx, project, after, limit, include_archived)
    }
    pub fn change_repository(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        change: &RepositoryChange,
    ) -> Result<ProjectRepository, PlatformError> {
        RepositoryStore::new(self).change(ctx, project, id, change)
    }
    pub fn issue_key(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        name: &str,
        scope: KeyScope,
        lifetime: i64,
    ) -> Result<IssuedApiKey, PlatformError> {
        CredentialStore::new(self).issue(ctx, actor, name, scope, lifetime)
    }
    pub fn list_keys(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
    ) -> Result<Vec<ApiKeyMetadata>, PlatformError> {
        CredentialStore::new(self).list(ctx, actor)
    }
    pub fn revoke_key(&mut self, ctx: &AuthContext, id: &str) -> Result<(), PlatformError> {
        CredentialStore::new(self).revoke(ctx, id)
    }
    pub fn rotate_key(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        lifetime: i64,
    ) -> Result<IssuedApiKey, PlatformError> {
        CredentialStore::new(self).rotate(ctx, id, lifetime)
    }
}
