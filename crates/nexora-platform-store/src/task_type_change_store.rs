use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct TaskTypeChangeStore;
impl TaskTypeChangeStore {
    pub fn change(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        code: &str,
        input: &TaskTypeChange,
    ) -> Result<TaskType, PlatformError> {
        if input.expected_revision < 1
            || (input.name.is_none()
                && input.description.is_none()
                && input.archived.is_none()
                && input.color.is_unchanged())
        {
            return Err(PlatformError::Validation(
                "task type change requires revision and fields",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let name = input.name.as_deref().map(DomainText::name).transpose()?;
        let description = input
            .description
            .as_deref()
            .map(|value| DomainText::required(value, 2000))
            .transpose()?;
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::TasksWrite, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
        let shared_change =
            input.name.is_some() || input.description.is_some() || !input.color.is_unchanged();
        if shared_change {
            PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
            tx.query_opt(
                "SELECT code FROM task_type_catalog WHERE code=$1 FOR UPDATE",
                &[&code],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "task_type.change",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(t)||jsonb_build_object('icon',(SELECT c.icon FROM task_type_catalog c WHERE c.code=t.code)) FROM task_types t WHERE project_id=$1 AND code=$2 FOR UPDATE",
                &[&project, &code],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let before: TaskType = PlatformStore::decode(value)?;
        if before.revision != input.expected_revision {
            return Err(PlatformError::Conflict("task type revision changed"));
        }
        let color = input.color.apply(before.color.clone());
        let value:Value=tx.query_one("UPDATE task_types AS t SET name=COALESCE($3,name),description=COALESCE($4,description),archived=COALESCE($5,archived),color=$6,revision=revision+1 WHERE project_id=$1 AND code=$2 RETURNING to_jsonb(t)||jsonb_build_object('icon',(SELECT c.icon FROM task_type_catalog c WHERE c.code=t.code))",&[&project,&code,&name,&description,&input.archived,&color.as_ref().map(HexColor::as_str)]).map_err(PlatformStore::database_error)?.get(0);
        if shared_change {
            tx.execute("UPDATE task_type_catalog SET name=COALESCE($2,name),description=COALESCE($3,description),color=$4,revision=revision+1 WHERE code=$1", &[&code,&name,&description,&color.as_ref().map(HexColor::as_str)]).map_err(PlatformStore::database_error)?;
            tx.execute("UPDATE task_types SET name=COALESCE($3,name),description=COALESCE($4,description),color=$5,revision=revision+1 WHERE code=$1 AND project_id<>$2", &[&code,&project,&name,&description,&color.as_ref().map(HexColor::as_str)]).map_err(PlatformStore::database_error)?;
        }
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "task_type.changed",
            Some(code),
            json!({"project_id":project,"before":before,"after":value,"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
