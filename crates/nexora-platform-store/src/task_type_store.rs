use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};

pub(crate) struct TaskTypeStore;
impl TaskTypeStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &TaskTypeCreate,
    ) -> Result<TaskType, PlatformError> {
        let code = input.code.trim().to_ascii_lowercase();
        if code.is_empty()
            || code.len() > 64
            || !code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(PlatformError::Validation("invalid task type code"));
        }
        let name = DomainText::name(&input.name)?;
        let color = input.color.as_ref().map(HexColor::as_str);
        let description = input
            .description
            .as_deref()
            .map(|v| DomainText::required(v, 2000))
            .transpose()?;
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
            "task_type.create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        if tx
            .query_opt(
                "SELECT code FROM task_types WHERE project_id=$1 AND code=$2",
                &[&project, &code],
            )
            .map_err(PlatformStore::database_error)?
            .is_some()
        {
            return Err(PlatformError::Conflict("task type already exists"));
        }
        let catalog = tx
            .query_opt(
                "SELECT archived FROM task_type_catalog WHERE code=$1 FOR SHARE",
                &[&code],
            )
            .map_err(PlatformStore::database_error)?;
        if let Some(row) = catalog {
            if row.get::<_, bool>(0) {
                return Err(PlatformError::Validation("catalog task type is archived"));
            }
        } else {
            PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
            tx.execute(
                "INSERT INTO task_type_catalog(code,name,description,color,purpose) VALUES($1,$2,$3,$4,$5)",
                &[&code, &name, &description, &color, &input.purpose.as_str()],
            )
            .map_err(PlatformStore::database_error)?;
        }
        let value:Value=tx.query_one("INSERT INTO task_types AS t(project_id,code,name,description,color) SELECT $1,code,name,description,color FROM task_type_catalog WHERE code=$2 RETURNING to_jsonb(t)||jsonb_build_object('icon',(SELECT c.icon FROM task_type_catalog c WHERE c.code=t.code),'purpose',(SELECT c.purpose FROM task_type_catalog c WHERE c.code=t.code))",&[&project,&code]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "task_type.created",
            Some(&code),
            json!({"project_id":project,"color":input.color}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
