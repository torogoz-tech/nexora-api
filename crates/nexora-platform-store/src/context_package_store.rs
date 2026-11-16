use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    context_package_capture::ContextPackageCapture, context_package_policy::ContextPackagePolicy,
    document_policy_store::DocumentPolicyStore, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct ContextPackageStore;
impl ContextPackageStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &ProjectContextPackageCreate,
    ) -> Result<ProjectContextPackage, PlatformError> {
        let name = DomainText::name(&input.name)?;
        let reason = DomainText::required(&input.reason, 4000)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsWrite)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
        if !input.task_ids.is_empty() {
            PlatformAuthority::require(&mut tx, ctx, Capability::TasksRead, Some(project))?;
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "context_package.create",
        )? {
            let cached: ProjectContextPackage = PlatformStore::decode(value)?;
            let result = ContextPackagePolicy::get(&mut tx, ctx, project, &cached.id)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(result);
        }
        let content = ContextPackageCapture::capture(&mut tx, ctx, project, input)?;
        let raw = serde_json::to_value(&content).map_err(|_| PlatformError::Storage)?;
        if serde_json::to_vec(&raw)
            .map_err(|_| PlatformError::Storage)?
            .len()
            > 524288
        {
            return Err(PlatformError::Validation(
                "context package exceeds content budget",
            ));
        }
        let row = tx
            .query_one("SELECT $1::jsonb,octet_length(($1::jsonb)::text)", &[&raw])
            .map_err(PlatformStore::database_error)?;
        if row.get::<_, i32>(1) > 1048576 {
            return Err(PlatformError::Validation(
                "stored context package exceeds budget",
            ));
        }
        let raw: Value = row.get(0);
        if serde_json::to_vec(&raw)
            .map_err(|_| PlatformError::Storage)?
            .len()
            > 524288
        {
            return Err(PlatformError::Validation(
                "normalized context package exceeds budget",
            ));
        }
        let mut value = ProjectContextPackage {
            id: SecretGenerator::id("ctxp")?,
            project_id: project.into(),
            name,
            content: PlatformStore::decode(raw.clone())?,
            checksum: String::new(),
            reason,
            created_by: ctx.actor_id().into(),
            created_at: String::new(),
            needs_review: false,
        };
        value.checksum = ContextPackagePolicy::checksum(&value)?;
        tx.execute("INSERT INTO project_context_packages(id,project_id,name,content,checksum,reason,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)",&[&value.id,&project,&value.name,&raw,&value.checksum,&value.reason,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "context_package.created",
            Some(&value.id),
            json!({"project_id":project,"checksum":value.checksum,"documents":value.content.documents.len(),"tasks":value.content.tasks.len(),"reason":value.reason}),
        )?;
        let result = ContextPackagePolicy::get(&mut tx, ctx, project, &value.id)?;
        ReceiptWriter::save(
            &mut tx,
            request.as_ref(),
            &serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?,
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ProjectContextPackage, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let result = ContextPackagePolicy::get(&mut tx, ctx, project, id)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
