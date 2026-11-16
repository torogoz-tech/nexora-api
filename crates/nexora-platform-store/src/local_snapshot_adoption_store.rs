use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    local_snapshot_publication_store::LocalSnapshotPublicationStore,
    local_snapshot_verifier::LocalSnapshotVerifier,
    local_source_operator_authority::LocalSourceOperatorAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::{
    AuthContext, DomainText, LocalSnapshotAdoption, LocalSnapshotPublication, PlatformError,
};
use serde_json::{Value, json};

pub(crate) struct LocalSnapshotAdoptionStore;
impl LocalSnapshotAdoptionStore {
    pub fn adopt(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &LocalSnapshotAdoption,
    ) -> Result<LocalSnapshotPublication, PlatformError> {
        Self::publish(store, ctx, project, input, false)
    }
    pub fn reauthorize(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &LocalSnapshotAdoption,
    ) -> Result<LocalSnapshotPublication, PlatformError> {
        Self::publish(store, ctx, project, input, true)
    }
    fn publish(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &LocalSnapshotAdoption,
        reauthorize: bool,
    ) -> Result<LocalSnapshotPublication, PlatformError> {
        let operation = if reauthorize {
            "local_source.reauthorize"
        } else {
            "local_snapshot.adopt"
        };
        let request = store
            .request_context
            .clone()
            .ok_or(PlatformError::Validation(
                "local adoption requires an idempotency key",
            ))?;
        // Release platform locks before loading the complete persisted payload/projection.
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        LocalSourceOperatorAuthority::lock(&mut tx, ctx, project)?;
        input.validate()?;
        if reauthorize && input.repository_id.is_none() {
            return Err(PlatformError::Validation(
                "reauthorization requires an existing local source",
            ));
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            Some(&request),
            ctx.actor_id(),
            operation,
        )? {
            let previous: LocalSnapshotPublication = PlatformStore::decode(value)?;
            if previous.project_id != project {
                return Err(PlatformError::Conflict("local adoption project changed"));
            }
            LocalSnapshotPublicationStore::lock_receipt(&mut tx, &previous)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(previous);
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        let verified = LocalSnapshotVerifier::verify(store, input)?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        LocalSourceOperatorAuthority::lock(&mut tx, ctx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            Some(&request),
            ctx.actor_id(),
            operation,
        )? {
            let previous: LocalSnapshotPublication = PlatformStore::decode(value)?;
            if previous.project_id != project {
                return Err(PlatformError::Conflict("local adoption project changed"));
            }
            LocalSnapshotPublicationStore::lock_receipt(&mut tx, &previous)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(previous);
        }
        let publication = SecretGenerator::id("lpub")?;
        let name = DomainText::name(&input.name)?;
        let reason = DomainText::required(&input.reason, 4000)?;
        let (source, repository, epoch, binding_revision) = if let Some(repository) =
            &input.repository_id
        {
            let row = tx.query_opt("SELECT id,owner_actor_id,revision,authorization_revision,binding_revision,runtime_tenant_id,runtime_repository_id,state FROM local_repository_sources WHERE repository_id=$1 AND project_id=$2 FOR UPDATE", &[&repository,&project])
                .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
            let source: String = row.get(0);
            let owner: String = row.get(1);
            let revision: i64 = row.get(2);
            let epoch: i64 = row.get(3);
            let binding: i64 = row.get(4);
            let state: String = row.get(7);
            if !reauthorize && state != "active" {
                return Err(PlatformError::Conflict(
                    "revoked local sources require explicit reauthorization",
                ));
            }
            if reauthorize && state != "revoked" {
                return Err(PlatformError::Conflict(
                    "only revoked local sources can be reauthorized",
                ));
            }
            if Some(revision) != input.expected_revision {
                return Err(PlatformError::Conflict("local source revision changed"));
            }
            if row.get::<_, String>(5) != verified.tenant_id.0
                || row.get::<_, String>(6) != verified.repository_id.0
            {
                return Err(PlatformError::Conflict(
                    "local source identity cannot be replaced",
                ));
            }
            tx.query_opt(
                "SELECT id FROM actors WHERE id=$1 AND active FOR SHARE",
                &[&owner],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Conflict("local source owner is inactive"))?;
            let expected_status = if reauthorize { "revoked" } else { "authorized" };
            tx.query_opt("SELECT id FROM project_repositories WHERE id=$1 AND project_id=$2 AND source_kind='local_snapshot' AND NOT archived AND authorization_status=$3 FOR UPDATE", &[&repository,&project,&expected_status])
                .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
            (
                source,
                repository.clone(),
                epoch + i64::from(reauthorize),
                binding + 1,
            )
        } else {
            let source = SecretGenerator::id("lsrc")?;
            let repository = SecretGenerator::id("repo")?;
            tx.execute("INSERT INTO project_repositories(id,project_id,source_kind,local_name,authorization_status) VALUES($1,$2,'local_snapshot',$3,'authorized')", &[&repository,&project,&name]).map_err(PlatformStore::database_error)?;
            tx.execute("INSERT INTO local_repository_sources(id,repository_id,project_id,runtime_tenant_id,runtime_repository_id,owner_actor_id,current_publication_id) VALUES($1,$2,$3,$4,$5,$6,$7)", &[&source,&repository,&project,&verified.tenant_id.0,&verified.repository_id.0,&ctx.actor_id(),&publication]).map_err(PlatformStore::database_error)?;
            (source, repository, 1, 1)
        };
        tx.execute("INSERT INTO local_snapshot_publications(id,source_id,authorization_revision,snapshot_id,payload_hash,binding_revision,attested_by,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8)", &[&publication,&source,&epoch,&verified.id.0,&verified.payload_hash,&binding_revision,&ctx.actor_id(),&reason]).map_err(PlatformStore::database_error)?;
        if input.repository_id.is_some() {
            if reauthorize {
                let generation = epoch - 1;
                tx.execute("UPDATE local_repository_sources SET state='active',authorization_revision=$4,source_generation=$5,current_publication_id=$2,binding_revision=$3,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1", &[&source,&publication,&binding_revision,&epoch,&generation]).map_err(PlatformStore::database_error)?;
                tx.execute("UPDATE project_repositories SET local_name=$2,authorization_status='authorized',source_generation=$3,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1", &[&repository,&name,&generation]).map_err(PlatformStore::database_error)?;
            } else {
                tx.execute("UPDATE local_repository_sources SET current_publication_id=$2,binding_revision=$3,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1", &[&source,&publication,&binding_revision]).map_err(PlatformStore::database_error)?;
                tx.execute("UPDATE project_repositories SET local_name=$2,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1", &[&repository,&name]).map_err(PlatformStore::database_error)?;
            }
        }
        let result = LocalSnapshotPublicationStore::get(&mut tx, &source, &publication)?;
        let response: Value = serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            if reauthorize {
                "local_source.reauthorized"
            } else {
                "local_snapshot.adopted"
            },
            Some(&repository),
            json!({"project_id":project,"source_id":source,"publication_id":publication,
                "source_revision":result.source_revision,"authorization_revision":epoch,
                "snapshot_id":verified.id.0,"payload_hash":verified.payload_hash,"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, Some(&request), &response)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
