use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) struct WorkflowDefinitionStore;
impl WorkflowDefinitionStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        input: &WorkflowCreate,
    ) -> Result<WorkflowDefinition, PlatformError> {
        let name = DomainText::name(&input.name)?;
        let purpose = DomainText::optional(&input.purpose, 4000)?;
        let color = crate::workflow_color::workflow_color(input.color.as_deref())?;
        Self::kinds(&input.resource_kinds)?;
        if project.is_none() && input.resource_kinds != [WorkflowResourceKind::Task] {
            return Err(PlatformError::Validation(
                "developer workflows govern tasks only",
            ));
        }
        input.graph.validate()?;
        input.graph.validate_resource_kinds(&input.resource_kinds)?;
        let kinds =
            serde_json::to_value(&input.resource_kinds).map_err(|_| PlatformError::Storage)?;
        let draft = serde_json::to_value(&input.graph).map_err(|_| PlatformError::Storage)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::WorkflowsWrite, project)?;
        if let Some(project) = project {
            PlatformAuthority::project(&mut tx, project)?;
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "workflow.create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let id = SecretGenerator::id("wfl")?;
        let value:Value=tx.query_one("INSERT INTO workflows AS w(id,project_id,name,purpose,resource_kinds,draft,created_by,icon,color) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING to_jsonb(w)",&[&id,&project,&name,&purpose,&kinds,&draft,&ctx.actor_id(),&input.icon.as_str(),&color]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "workflow.created",
            Some(&id),
            json!({"project_id":project,"revision":1}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn change(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
        input: &WorkflowChange,
    ) -> Result<WorkflowDefinition, PlatformError> {
        if input.expected_revision < 1
            || (input.name.is_none()
                && input.purpose.is_none()
                && input.icon.is_none()
                && input.color.is_none()
                && input.resource_kinds.is_none()
                && input.graph.is_none()
                && input.archived.is_none())
        {
            return Err(PlatformError::Validation(
                "workflow change requires a revision and changed fields",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let name = input.name.as_deref().map(DomainText::name).transpose()?;
        let purpose = input
            .purpose
            .as_deref()
            .map(|text| DomainText::optional(text, 4000))
            .transpose()?;
        if project.is_none()
            && input
                .resource_kinds
                .as_ref()
                .is_some_and(|kinds| kinds != &[WorkflowResourceKind::Task])
        {
            return Err(PlatformError::Validation(
                "developer workflows govern tasks only",
            ));
        }
        if let Some(kinds) = &input.resource_kinds {
            Self::kinds(kinds)?;
        }
        if let Some(graph) = &input.graph {
            graph.validate()?;
        }
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::WorkflowsWrite, project)?;
        if let Some(project) = project {
            PlatformAuthority::project(&mut tx, project)?;
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "workflow.change",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(w) FROM workflows w WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2 FOR UPDATE",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let mut current: WorkflowDefinition = PlatformStore::decode(value)?;
        if current.revision != input.expected_revision {
            return Err(PlatformError::Conflict("workflow revision changed"));
        }
        if let Some(name) = name {
            current.name = name;
        }
        if let Some(icon) = input.icon {
            current.icon = icon;
        }
        if let Some(color) = &input.color {
            current.color = crate::workflow_color::workflow_color(Some(color))?;
        }
        if let Some(purpose) = purpose {
            current.purpose = purpose;
        }
        if let Some(kinds) = &input.resource_kinds {
            current.resource_kinds = kinds.clone();
        }
        if let Some(graph) = &input.graph {
            current.draft = graph.clone();
        }
        if let Some(archived) = input.archived {
            current.archived = archived;
        }
        current
            .draft
            .validate_resource_kinds(&current.resource_kinds)?;
        let kinds =
            serde_json::to_value(&current.resource_kinds).map_err(|_| PlatformError::Storage)?;
        let draft = serde_json::to_value(&current.draft).map_err(|_| PlatformError::Storage)?;
        let value:Value=tx.query_one("UPDATE workflows AS w SET name=$3,purpose=$4,resource_kinds=$5,draft=$6,archived=$7,icon=$8,color=$9,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2 RETURNING to_jsonb(w)",&[&id,&project,&current.name,&current.purpose,&kinds,&draft,&current.archived,&current.icon.as_str(),&current.color]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "workflow.changed",
            Some(id),
            json!({"project_id":project,"revision":value["revision"],"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub(crate) fn kinds(kinds: &[WorkflowResourceKind]) -> Result<(), PlatformError> {
        if kinds.is_empty()
            || kinds.len() > 5
            || kinds.iter().collect::<BTreeSet<_>>().len() != kinds.len()
        {
            return Err(PlatformError::Validation(
                "workflow resource kinds must be nonempty and unique",
            ));
        }
        Ok(())
    }
}
