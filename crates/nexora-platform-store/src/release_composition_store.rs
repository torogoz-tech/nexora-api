use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter, receipt_writer::ReceiptWriter,
    task_release_store::TaskReleaseStore,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn queue_release_composition(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        release: &str,
        revision: i64,
    ) -> Result<Value, PlatformError> {
        TaskReleaseStore::authorize_publication(self, ctx, project)?;
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "release.compose",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        let row=tx.query_opt("SELECT revision,status FROM task_releases WHERE id=$1 AND project_id=$2 FOR UPDATE",&[&release,&project]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, i64>(0) != revision || row.get::<_, String>(1) != "prepared" {
            return Err(PlatformError::Conflict(
                "release changed before composition",
            ));
        }
        let composed:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_release_repositories WHERE release_id=$1) AND NOT EXISTS(SELECT 1 FROM task_release_repositories WHERE release_id=$1 AND composition_status<>'composed')",&[&release]).map_err(Self::database_error)?.get(0);
        if composed {
            return Err(PlatformError::Conflict(
                "release branch is already composed; continue its approved delivery pipeline",
            ));
        }
        let repos = tx
            .query(
                "SELECT repository_id FROM task_release_repositories WHERE release_id=$1",
                &[&release],
            )
            .map_err(Self::database_error)?;
        for repo in repos {
            let repository: String = repo.get(0);
            if !tx.query_one("SELECT EXISTS(SELECT 1 FROM task_release_commit_pins WHERE release_id=$1 AND repository_id=$2)",&[&release,&repository]).map_err(Self::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("every release repository requires explicit verified task commits"));}
        }
        if tx.query_one("SELECT EXISTS(SELECT 1 FROM task_release_commit_pins WHERE release_id=$1 GROUP BY repository_id HAVING count(DISTINCT commit_sha)>100)",&[&release]).map_err(Self::database_error)?.get::<_,bool>(0){return Err(PlatformError::Validation("release supports at most 100 explicit commits per repository"));}
        let id = SecretGenerator::id("rcj")?;
        let method = if ctx.method() == AuthMethod::ApiKey {
            "api_key"
        } else {
            "session"
        };
        let value:Value=tx.query_one("INSERT INTO task_release_composition_jobs AS j(id,release_id,project_id,release_revision,actor_id,credential_id,auth_method) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(release_id,release_revision) DO UPDATE SET status=CASE WHEN j.status='failed' THEN 'queued' ELSE j.status END,actor_id=CASE WHEN j.status='failed' THEN EXCLUDED.actor_id ELSE j.actor_id END,credential_id=CASE WHEN j.status='failed' THEN EXCLUDED.credential_id ELSE j.credential_id END,auth_method=CASE WHEN j.status='failed' THEN EXCLUDED.auth_method ELSE j.auth_method END,updated_at=clock_timestamp() RETURNING to_jsonb(j)-'credential_id'",&[&id,&release,&project,&revision,&ctx.actor_id(),&ctx.credential_id(),&method]).map_err(Self::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "release.composition_queued",
            Some(release),
            json!({"project_id":project,"job_id":value["id"],"release_revision":revision}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn release_composition_jobs(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        release: &str,
    ) -> Result<Value, PlatformError> {
        self.task_release(ctx, project, release)?;
        let items:Vec<Value>=self.client.query("SELECT to_jsonb(j)-'credential_id' FROM task_release_composition_jobs j WHERE release_id=$1 AND project_id=$2 ORDER BY created_at DESC LIMIT 20",&[&release,&project]).map_err(Self::database_error)?.into_iter().map(|row|row.get(0)).collect();
        Ok(json!({"items":items}))
    }
}
