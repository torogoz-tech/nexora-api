use crate::PlatformStore;
use nexora_platform::*;
impl PlatformStore {
    pub fn actor_workflows(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
    ) -> Result<Vec<DeveloperWorkflowAssignment>, PlatformError> {
        crate::developer_workflow_store::DeveloperWorkflowStore::list(self, ctx, actor)
    }

    pub fn remove_actor_workflow(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        input: &DeveloperWorkflowRemoval,
    ) -> Result<(), PlatformError> {
        crate::developer_workflow_store::DeveloperWorkflowStore::remove(self, ctx, actor, input)
    }

    pub fn create_developer_task(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        input: &TaskCreate,
    ) -> Result<WorkTask, PlatformError> {
        crate::task_definition_store::TaskDefinitionStore::create(self, ctx, project, input, true)
    }

    pub fn developer_directory(
        &mut self,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
    ) -> Result<DeveloperDirectoryPage, PlatformError> {
        crate::developer_workflow_store::DeveloperWorkflowStore::directory(self, ctx, after, limit)
    }
    pub fn developer_board_tasks(
        &mut self,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
    ) -> Result<DeveloperBoardPage, PlatformError> {
        crate::developer_board_store::DeveloperBoardStore::page(self, ctx, after, limit)
    }

    pub fn developer_workflow_page(
        &mut self,
        ctx: &AuthContext,
        after: Option<&str>,
        limit: u16,
        archived: bool,
    ) -> Result<WorkflowPage, PlatformError> {
        crate::workflow_query_store::WorkflowQueryStore::page(
            self, ctx, None, after, limit, archived,
        )
    }
    pub fn developer_workflow(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<WorkflowDefinition, PlatformError> {
        crate::workflow_query_store::WorkflowQueryStore::get(self, ctx, None, id)
    }
    pub fn create_developer_workflow(
        &mut self,
        ctx: &AuthContext,
        input: &DeveloperWorkflowCreate,
    ) -> Result<WorkflowDefinition, PlatformError> {
        crate::workflow_definition_store::WorkflowDefinitionStore::create(
            self,
            ctx,
            None,
            &WorkflowCreate {
                name: input.name.clone(),
                icon: input.icon,
                color: input.color.clone(),
                purpose: input.purpose.clone(),
                graph: input.graph.clone(),
                resource_kinds: vec![WorkflowResourceKind::Task],
            },
        )
    }
    pub fn change_developer_workflow(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &DeveloperWorkflowEdit,
    ) -> Result<WorkflowDefinition, PlatformError> {
        crate::workflow_definition_store::WorkflowDefinitionStore::change(
            self,
            ctx,
            None,
            id,
            &WorkflowChange {
                expected_revision: input.expected_revision,
                name: input.name.clone(),
                icon: input.icon,
                color: input.color.clone(),
                purpose: input.purpose.clone(),
                graph: input.graph.clone(),
                resource_kinds: None,
                archived: input.archived,
                reason: input.reason.clone(),
            },
        )
    }
    pub fn publish_developer_workflow(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &WorkflowPublish,
    ) -> Result<WorkflowVersion, PlatformError> {
        crate::workflow_publication_store::WorkflowPublicationStore::publish(
            self, ctx, None, id, input,
        )
    }
    pub fn developer_workflow_versions(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        before: Option<i64>,
        limit: u16,
    ) -> Result<WorkflowVersionPage, PlatformError> {
        crate::workflow_query_store::WorkflowQueryStore::versions(
            self, ctx, None, id, before, limit,
        )
    }
    pub fn developer_workflow_version(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        version: &str,
    ) -> Result<WorkflowVersion, PlatformError> {
        crate::workflow_query_store::WorkflowQueryStore::version(self, ctx, None, id, version)
    }
    pub fn developer_workflow_members(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Vec<DeveloperWorkflowAssignment>, PlatformError> {
        crate::developer_workflow_store::DeveloperWorkflowStore::members(self, ctx, id)
    }
    pub fn actor_workflow(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
    ) -> Result<Option<DeveloperWorkflowAssignment>, PlatformError> {
        crate::developer_workflow_store::DeveloperWorkflowStore::get(self, ctx, actor)
    }
    pub fn assign_actor_workflow(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        input: &DeveloperWorkflowChange,
    ) -> Result<DeveloperWorkflowAssignment, PlatformError> {
        crate::developer_workflow_store::DeveloperWorkflowStore::assign(self, ctx, actor, input)
    }
}
