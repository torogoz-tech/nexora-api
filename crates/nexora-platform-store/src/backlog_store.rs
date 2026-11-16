use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
    task_policy_store::TaskPolicyStore,
};
use nexora_platform::*;
use serde_json::{Value, json};

pub(crate) struct BacklogStore;
impl BacklogStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &BacklogCreate,
    ) -> Result<Backlog, PlatformError> {
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
        PlatformAuthority::require(&mut tx, ctx, Capability::BacklogsWrite, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "backlog.create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let flow = TaskPolicyStore::flow(&mut tx, project, &input.workflow_version_id, true)?;
        let id = SecretGenerator::id("bkl")?;
        let value:Value=tx.query_one("INSERT INTO backlogs AS b(id,project_id,name,description,workflow_id,workflow_version_id,created_by,color) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING to_jsonb(b)",&[&id,&project,&name,&description,&flow.workflow_id,&flow.id,&ctx.actor_id(),&color]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "backlog.created",
            Some(&id),
            json!({"project_id":project,"workflow_version_id":flow.id,"color":input.color}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn change(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &BacklogChange,
    ) -> Result<Backlog, PlatformError> {
        if input.expected_revision < 1
            || (input.name.is_none()
                && input.description.is_none()
                && input.workflow_version_id.is_none()
                && input.archived.is_none()
                && input.color.is_unchanged())
        {
            return Err(PlatformError::Validation(
                "backlog change requires revision and fields",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let name = input.name.as_deref().map(DomainText::name).transpose()?;
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
        PlatformAuthority::require(&mut tx, ctx, Capability::BacklogsWrite, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "backlog.change",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let mut current = TaskPolicyStore::backlog(&mut tx, project, id)?;
        if current.revision != input.expected_revision {
            return Err(PlatformError::Conflict("backlog revision changed"));
        }
        let previous_color = current.color.clone();
        current.color = input.color.apply(current.color);
        if let Some(name) = name {
            current.name = name;
        }
        if let Some(description) = description {
            current.description = Some(description);
        }
        if let Some(version) = &input.workflow_version_id {
            let flow = TaskPolicyStore::flow(&mut tx, project, version, true)?;
            current.workflow_id = flow.workflow_id;
            current.workflow_version_id = flow.id;
        }
        if let Some(archived) = input.archived {
            current.archived = archived;
        }
        let value:Value=tx.query_one("UPDATE backlogs AS b SET name=$3,description=$4,workflow_id=$5,workflow_version_id=$6,archived=$7,color=$8,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2 RETURNING to_jsonb(b)",&[&id,&project,&current.name,&current.description,&current.workflow_id,&current.workflow_version_id,&current.archived,&current.color.as_ref().map(HexColor::as_str)]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "backlog.changed",
            Some(id),
            json!({"project_id":project,"revision":value["revision"],"reason":reason,"previous_color":previous_color,"color":current.color}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<Backlog, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::TasksRead, Some(project)).map_err(
            |e| {
                if matches!(e, PlatformError::Forbidden) {
                    PlatformError::NotFound
                } else {
                    e
                }
            },
        )?;
        PlatformAuthority::project(&mut tx, project)?;
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(b) FROM backlogs b WHERE id=$1 AND project_id=$2",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
