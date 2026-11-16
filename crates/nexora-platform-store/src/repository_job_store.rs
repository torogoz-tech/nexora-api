use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};

pub(crate) struct RepositoryJobStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> RepositoryJobStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn enqueue(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        kind: RepositoryJobKind,
        input: &RepositoryJobRequest,
    ) -> Result<RepositoryJob, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "expected revision must be positive",
            ));
        }
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::RepositoriesSync, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            kind.operation(),
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let source = crate::repository_job_source::RepositoryJobSource::lock(
            &mut tx,
            ctx,
            project,
            repository,
            input.expected_revision,
        )?;
        let connection = source.connection;
        let connection_revision = source.connection_revision;
        let revision = source.repository_revision;
        let generation = source.generation;
        let id = SecretGenerator::id("job")?;
        let method = if ctx.method() == AuthMethod::ApiKey {
            "api_key"
        } else {
            "session"
        };
        let value: Value = tx.query_one("INSERT INTO repository_jobs AS j(id,project_id,repository_id,requested_by,credential_id,credential_method,kind,repository_revision,source_generation,connection_id,connection_revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING to_jsonb(j)", &[&id,&project,&repository,&ctx.actor_id(),&ctx.credential_id(),&method,&kind.as_str(),&revision,&generation,&connection,&connection_revision]).map_err(PlatformStore::database_error)?.get(0);
        let job: RepositoryJob = PlatformStore::decode(value)?;
        let public = serde_json::to_value(&job).map_err(|_| PlatformError::Storage)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "repository.job_enqueued",
            Some(&id),
            json!({"project_id":project,"repository_id":repository,"kind":kind,"repository_revision":revision,"source_generation":generation}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &public)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(job)
    }
    pub fn get(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<RepositoryJob, PlatformError> {
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::RepositoriesRead, Some(project))
            .map_err(|e| match e {
                PlatformError::Forbidden => PlatformError::NotFound,
                other => other,
            })?;
        PlatformAuthority::project(&mut tx, project)?;
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(j) FROM repository_jobs j WHERE id=$1 AND project_id=$2",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let job = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(job)
    }
    pub fn cancel(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &RepositoryJobChange,
    ) -> Result<RepositoryJob, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "expected revision must be positive",
            ));
        }
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::RepositoriesSync, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "repository.job.cancel",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let row = tx.query_opt("SELECT revision,state FROM repository_jobs WHERE id=$1 AND project_id=$2 FOR UPDATE", &[&id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, i64>(0) != input.expected_revision {
            return Err(PlatformError::Conflict("job revision changed"));
        }
        if !matches!(row.get::<_, String>(1).as_str(), "queued" | "running") {
            return Err(PlatformError::Conflict("job is already terminal"));
        }
        let value: Value = tx.query_one("UPDATE repository_jobs AS j SET state='cancelled',lease_owner=NULL,lease_token_digest=NULL,lease_until=NULL,revision=revision+1,updated_at=clock_timestamp(),finished_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(j)", &[&id]).map_err(PlatformStore::database_error)?.get(0);
        let job: RepositoryJob = PlatformStore::decode(value)?;
        let public = serde_json::to_value(&job).map_err(|_| PlatformError::Storage)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "repository.job_cancelled",
            Some(id),
            json!({"project_id":project,"revision":job.revision}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &public)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(job)
    }
}
