use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
    task_template_policy_store::TaskTemplatePolicyStore,
};
use nexora_platform::*;
use serde_json::{Value, json};

pub(crate) struct TaskTemplateDefinitionStore;
impl TaskTemplateDefinitionStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &TaskTemplateCreate,
    ) -> Result<TaskTemplate, PlatformError> {
        let code = input.code.trim().to_ascii_lowercase();
        if code.is_empty()
            || code.len() > 64
            || !code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
        {
            return Err(PlatformError::Validation("invalid template code"));
        }
        let name = DomainText::name(&input.name)?;
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
            "task_template.create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let id = SecretGenerator::id("ttp")?;
        let row=tx.query_opt("INSERT INTO task_templates AS t(id,project_id,code,name,created_by) VALUES($1,$2,$3,$4,$5) ON CONFLICT(project_id,code) DO NOTHING RETURNING to_jsonb(t)",&[&id,&project,&code,&name,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("template code already exists"))?;
        let value: Value = row.get(0);
        let result = PlatformStore::decode(value.clone())?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "task_template.created",
            Some(&id),
            json!({"project_id":project,"code":code}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn change(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskTemplateChange,
    ) -> Result<TaskTemplate, PlatformError> {
        if input.expected_revision < 1 || (input.name.is_none() && input.archived.is_none()) {
            return Err(PlatformError::Validation(
                "template change requires revision and fields",
            ));
        }
        let name = input.name.as_deref().map(DomainText::name).transpose()?;
        let reason = DomainText::required(&input.reason, 4000)?;
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
            "task_template.change",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let before = TaskTemplatePolicyStore::definition(&mut tx, project, id)?;
        if before.revision != input.expected_revision {
            return Err(PlatformError::Conflict("template revision changed"));
        }
        let value:Value=tx.query_one("UPDATE task_templates AS t SET name=COALESCE($3,name),archived=COALESCE($4,archived),revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2 RETURNING to_jsonb(t)",&[&id,&project,&name,&input.archived]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "task_template.changed",
            Some(id),
            json!({"project_id":project,"revision":value["revision"],"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
