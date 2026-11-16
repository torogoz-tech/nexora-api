use crate::{PlatformStore, repository_snapshot_scope::RepositorySnapshotScope};
use crate::{
    audit_writer::AuditWriter, knowledge_receipt::KnowledgeReceipt,
    knowledge_request_identity::KnowledgeRequestIdentity, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::{AuthContext, Capability, PlatformError};
use nexora_runtime::{DatabaseRuntime, KnowledgeCommand};
use serde_json::{Value, json};

pub(crate) struct RepositoryKnowledgeStore;
impl RepositoryKnowledgeStore {
    pub fn query(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        snapshot: Option<&str>,
        command: &KnowledgeCommand,
    ) -> Result<Value, PlatformError> {
        command
            .validate()
            .map_err(|_| PlatformError::Validation("invalid knowledge command"))?;
        if command.is_write() {
            return Self::mutate(store, ctx, project, repository, snapshot, command);
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let scope = RepositorySnapshotScope::lock(&mut tx, ctx, project, repository, snapshot)?;
        if store.code_runtime.is_none() {
            store.code_runtime = Some(
                DatabaseRuntime::connect(&store.connections).map_err(|_| PlatformError::Storage)?,
            );
        }
        let runtime = store.code_runtime.as_mut().ok_or(PlatformError::Storage)?;
        let head = scope.verify(runtime)?;
        let mut value = runtime
            .knowledge(
                &head.tenant_id,
                &head.repository_id,
                Some(&head.id),
                ctx.actor_id(),
                command,
            )
            .map_err(|_| PlatformError::Storage)?;
        value["project_id"] = json!(project);
        value["repository_id"] = json!(repository);
        value["commit"] = json!(scope.commit);
        if let Some(origin) = &scope.local_origin {
            value["local_origin"] = json!(origin);
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
    fn mutate(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        snapshot: Option<&str>,
        command: &KnowledgeCommand,
    ) -> Result<Value, PlatformError> {
        let operation = KnowledgeRequestIdentity::operation(command)?;
        let request = store
            .request_context
            .clone()
            .ok_or(PlatformError::Validation(
                "knowledge mutation request required",
            ))?;
        if Some(request.request_id.as_str()) != command.request_id() {
            return Err(PlatformError::Conflict(
                "knowledge request ID does not match command",
            ));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::KnowledgeWrite, Some(project))?;
        tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('technical_lead','pmo','manager','super_admin') FOR SHARE",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Forbidden)?;
        let scope = RepositorySnapshotScope::lock(&mut tx, ctx, project, repository, snapshot)?;
        let authority = scope.authority(&mut tx)?;
        let receipt = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            Some(&request),
            ctx.actor_id(),
            operation,
        )?;
        if store.code_runtime.is_none() {
            store.code_runtime = Some(
                DatabaseRuntime::connect(&store.connections).map_err(|_| PlatformError::Storage)?,
            );
        }
        let runtime = store.code_runtime.as_mut().ok_or(PlatformError::Storage)?;
        if let Some(receipt) = receipt {
            let value = KnowledgeReceipt::replay(
                &mut tx, runtime, ctx, project, repository, &authority, receipt,
            )?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(value);
        }
        let head = scope.verify(runtime)?;
        let internal = KnowledgeRequestIdentity::internal(
            ctx.actor_id(),
            project,
            repository,
            operation,
            &request.request_id,
        )?;
        let command = KnowledgeRequestIdentity::command(command, internal);
        let mut value = runtime
            .knowledge_in_transaction(&mut tx, &head, ctx.actor_id(), &command)
            .map_err(|_| PlatformError::Storage)?;
        value["project_id"] = json!(project);
        value["repository_id"] = json!(repository);
        value["commit"] = json!(scope.commit);
        if let Some(origin) = &scope.local_origin {
            value["local_origin"] = json!(origin);
        }
        // Claims can contain NUL and live in the byte payload. Audit stores references only.
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            operation,
            Some(repository),
            json!({"project_id":project,"knowledge_id":value["contribution"]["id"],"revision":value["contribution"]["revision"],"snapshot":head.id.0}),
        )?;
        ReceiptWriter::save(
            &mut tx,
            Some(&request),
            &KnowledgeReceipt::encode(&authority, &head, &value),
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn pending(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        snapshot: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Value, PlatformError> {
        if offset > 1_000_000 || limit == 0 || limit > 100 {
            return Err(PlatformError::Validation("pending page outside bounds"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let scope = RepositorySnapshotScope::lock(&mut tx, ctx, project, repository, snapshot)?;
        if store.code_runtime.is_none() {
            store.code_runtime = Some(
                DatabaseRuntime::connect(&store.connections).map_err(|_| PlatformError::Storage)?,
            );
        }
        let runtime = store.code_runtime.as_mut().ok_or(PlatformError::Storage)?;
        let head = scope.verify(runtime)?;
        let mut value = runtime
            .knowledge_pending(
                &head.tenant_id,
                &head.repository_id,
                Some(&head.id),
                offset,
                limit,
            )
            .map_err(|_| PlatformError::Storage)?;
        value["project_id"] = json!(project);
        value["repository_id"] = json!(repository);
        value["commit"] = json!(scope.commit);
        if let Some(origin) = &scope.local_origin {
            value["local_origin"] = json!(origin);
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
}
