use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
    task_policy_store::TaskPolicyStore, task_template_checksum::TaskTemplateChecksum,
    task_template_policy_store::TaskTemplatePolicyStore,
    task_template_validation::TaskTemplateValidation,
};
use nexora_platform::*;
use serde_json::{Value, json};

pub(crate) struct TaskTemplatePublicationStore;
impl TaskTemplatePublicationStore {
    pub fn publish(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskTemplatePublish,
    ) -> Result<TaskTemplateVersion, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation("template revision required"));
        }
        let mut definition = TaskTemplateValidation::fields(&input.definition)?;
        let reason = DomainText::required(&input.reason, 4000)?;
        for value in [&input.valid_from, &input.valid_until]
            .into_iter()
            .flatten()
        {
            TaskTemplateValidation::timestamp(value)?;
        }
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
            "task_template.publish",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let template = TaskTemplatePolicyStore::definition(&mut tx, project, id)?;
        if template.revision != input.expected_revision || template.archived {
            return Err(PlatformError::Conflict(
                "template revision changed or archived",
            ));
        }
        TaskPolicyStore::kind(&mut tx, project, &definition.type_code)?;
        definition.fields = TaskPolicyStore::normalize_fields(&mut tx, &definition.fields)?
            .as_object()
            .ok_or(PlatformError::Storage)?
            .clone();
        // Normalize JSON through PostgreSQL before hashing so jsonb numeric/key
        // representation cannot invalidate an otherwise unchanged definition.
        let definition = serde_json::to_value(definition).map_err(|_| PlatformError::Storage)?;
        let row=tx.query_one("SELECT to_jsonb(COALESCE($1::text::timestamptz,clock_timestamp())),to_jsonb($2::text::timestamptz),$3::jsonb",&[&input.valid_from,&input.valid_until,&definition]).map_err(PlatformStore::database_error)?;
        let from: Value = row.get(0);
        let from = from.as_str().ok_or(PlatformError::Storage)?.to_owned();
        let until: Option<Value> = row.get(1);
        let until = until
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or(PlatformError::Storage)
            })
            .transpose()?;
        if let Some(until) = &until
            && TaskTemplateValidation::timestamp(until)?
                <= TaskTemplateValidation::timestamp(&from)?
        {
            return Err(PlatformError::Validation(
                "template validity interval is empty",
            ));
        }
        let definition: TaskTemplateFields =
            TaskTemplateValidation::fields(&PlatformStore::decode(row.get(2))?)?;
        let number = template
            .latest_version_number
            .checked_add(1)
            .ok_or(PlatformError::Conflict("template version exhausted"))?;
        let mut version = TaskTemplateVersion {
            id: SecretGenerator::id("ttv")?,
            project_id: project.into(),
            template_id: id.into(),
            number,
            previous_version_id: template.latest_version_id,
            code: template.code,
            name: template.name,
            definition,
            valid_from: from,
            valid_until: until,
            checksum: String::new(),
            reason,
            published_by: ctx.actor_id().into(),
            created_at: String::new(),
        };
        version.checksum = TaskTemplateChecksum::calculate(&version)?;
        let definition =
            serde_json::to_value(&version.definition).map_err(|_| PlatformError::Storage)?;
        let value:Value=tx.query_one("INSERT INTO task_template_versions AS v(id,project_id,template_id,number,previous_version_id,code,name,definition,valid_from,valid_until,checksum,reason,published_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9::text::timestamptz,$10::text::timestamptz,$11,$12,$13) RETURNING to_jsonb(v)",&[&version.id,&project,&id,&number,&version.previous_version_id,&version.code,&version.name,&definition,&version.valid_from,&version.valid_until,&version.checksum,&version.reason,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.get(0);
        tx.execute("UPDATE task_templates SET latest_version_id=$3,latest_version_number=$4,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2",&[&id,&project,&version.id,&number]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "task_template.published",
            Some(id),
            json!({"project_id":project,"version_id":version.id,"number":number,"checksum":version.checksum,"reason":version.reason}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
