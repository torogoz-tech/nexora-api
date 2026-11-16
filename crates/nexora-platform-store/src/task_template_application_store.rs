use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter, task_definition_store::TaskDefinitionStore,
    task_template_policy_store::TaskTemplatePolicyStore,
};
use nexora_platform::*;
use serde_json::json;

pub(crate) struct TaskTemplateApplicationStore;
impl TaskTemplateApplicationStore {
    pub fn apply(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &TaskTemplateApply,
    ) -> Result<WorkTask, PlatformError> {
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::TasksWrite, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.template_apply",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let version = TaskTemplatePolicyStore::applicable_version(
            &mut tx,
            project,
            &input.template_version_id,
        )?;
        let definition = &version.definition;
        let task = TaskCreate {
            entry_workflow_id: None,
            primary_requirement_id: None,
            primary_feature_id: None,
            acceptance_criteria: Vec::new(),
            title: input
                .title
                .clone()
                .or_else(|| definition.title.clone())
                .unwrap_or_default(),
            context: input
                .context
                .clone()
                .unwrap_or_else(|| definition.context.clone()),
            type_code: input
                .type_code
                .clone()
                .unwrap_or_else(|| definition.type_code.clone()),
            priority: input.priority.unwrap_or(definition.priority),
            fields: input
                .fields
                .clone()
                .unwrap_or_else(|| definition.fields.clone()),
            backlog_id: input.backlog_id.clone(),
            workflow_version_id: input.workflow_version_id.clone(),
            assignee_id: input.assignee_id.clone(),
            origin: input.origin.clone(),
            origin_reference: input.origin_reference.clone(),
        };
        let task = TaskDefinitionStore::insert(&mut tx, ctx, project, &task, Some(&version), true)?;
        tx.query_opt("SELECT id FROM task_template_versions WHERE id=$1 AND project_id=$2 AND valid_from<=clock_timestamp() AND (valid_until IS NULL OR valid_until>clock_timestamp())",&[&version.id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("template expired during application"))?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "task_template.applied",
            Some(&task.id),
            json!({"project_id":project,"template_id":version.template_id,"version_id":version.id,"checksum":version.checksum}),
        )?;
        let value = serde_json::to_value(&task).map_err(|_| PlatformError::Storage)?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(task)
    }
}
