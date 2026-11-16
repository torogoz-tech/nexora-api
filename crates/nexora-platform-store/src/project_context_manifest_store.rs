use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    document_policy_store::DocumentPolicyStore, project_context_checksum::ProjectContextChecksum,
    project_context_policy::ProjectContextPolicy, receipt_writer::ReceiptWriter,
    repository_snapshot_scope::RepositorySnapshotScope,
};
use nexora_platform::*;
use nexora_runtime::DatabaseRuntime;
use serde_json::{Value, json};
pub(crate) struct ProjectContextManifestStore;
impl ProjectContextManifestStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &ProjectContextManifestCreate,
    ) -> Result<ProjectContextManifest, PlatformError> {
        let name = DomainText::name(&input.name)?;
        let reason = DomainText::required(&input.reason, 4000)?;
        let mut ids = input.repository_ids.clone();
        ids.sort_unstable();
        if ids.is_empty()
            || ids.len() > 64
            || ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 128 || id.chars().any(char::is_control))
            || ids.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(PlatformError::Validation(
                "manifest requires1..64 distinct logical repositories",
            ));
        }
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsWrite)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "context_manifest.create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        if store.code_runtime.is_none() {
            store.code_runtime = Some(
                DatabaseRuntime::connect(&store.connections).map_err(|_| PlatformError::Storage)?,
            );
        }
        let runtime = store.code_runtime.as_mut().ok_or(PlatformError::Storage)?;
        let mut members = Vec::new();
        for id in ids {
            let scope = RepositorySnapshotScope::lock(&mut tx, ctx, project, &id, None)?;
            let head = scope.verify(runtime)?;
            let (connection_id, connection_revision) = scope.connection(&mut tx)?;
            members.push(ProjectContextMember {
                repository_id: id,
                snapshot_id: scope.snapshot,
                payload_hash: head.payload_hash,
                pinned_commit: scope.commit,
                source_generation: scope.source_generation,
                connection_id,
                connection_revision,
                binding_revision: scope.binding_revision,
                local_origin: scope.local_origin,
            });
        }
        let mut result = ProjectContextManifest {
            id: SecretGenerator::id("ctxm")?,
            project_id: project.into(),
            name,
            members,
            checksum: String::new(),
            reason,
            created_by: ctx.actor_id().into(),
            created_at: String::new(),
        };
        result.checksum = ProjectContextChecksum::manifest(&result)?;
        let members = serde_json::to_value(&result.members).map_err(|_| PlatformError::Storage)?;
        let value:Value=tx.query_one("INSERT INTO project_context_manifests AS m(id,project_id,name,members,checksum,reason,created_by) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING to_jsonb(m)",&[&result.id,&project,&result.name,&members,&result.checksum,&result.reason,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "context_manifest.created",
            Some(&result.id),
            json!({"project_id":project,"checksum":result.checksum,"repository_count":result.members.len(),"reason":result.reason}),
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
    ) -> Result<ProjectContextManifest, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
        let result = ProjectContextPolicy::get(&mut tx, project, id)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn status(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<ProjectContextStatus, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        DocumentPolicyStore::authorize(&mut tx, ctx, project, Capability::DocumentsRead)?;
        let manifest = ProjectContextPolicy::get(&mut tx, project, id)?;
        let result = ProjectContextPolicy::status(&mut tx, &manifest)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
