use crate::{
    GitNetworkPolicy, PlatformStore, audit_writer::AuditWriter,
    git_connection_store::GitConnectionStore, gitlab_client::GitlabClient,
    gitlab_token_store::GitlabTokenStore, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct GitRepositoryStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> GitRepositoryStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn authorize(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        input: &GitRepositoryAuthorization,
        policy: &GitNetworkPolicy,
    ) -> Result<ProjectRepository, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::RepositoriesWrite, Some(project))?;
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation(
                "expected revision must be positive",
            ));
        }
        if let Some(expected) = &input.expected_remote_id
            && (expected.len() > 20
                || expected
                    .parse::<u64>()
                    .ok()
                    .filter(|id| *id > 0)
                    .map(|id| id.to_string())
                    .as_deref()
                    != Some(expected.as_str()))
        {
            return Err(PlatformError::Validation(
                "invalid expected remote identity",
            ));
        }
        // A receipt describes a past operation, not current provider authority.
        // Read it before network/connection checks; final commit still locks and rechecks it.
        let cipher_request = self.store.request_context.clone();
        if cipher_request.is_some() {
            let mut tx = self
                .store
                .client
                .transaction()
                .map_err(PlatformStore::database_error)?;
            let previous = ReceiptWriter::read(
                &mut tx,
                &self.store.namespace,
                cipher_request.as_ref(),
                ctx.actor_id(),
                "repository.authorize",
            )?;
            tx.commit().map_err(PlatformStore::database_error)?;
            if let Some(value) = previous {
                return PlatformStore::decode(value);
            }
        }
        let row=self.store.client.query_opt("SELECT requested_full_name,provider,remote_id,source_kind FROM project_repositories WHERE id=$1 AND project_id=$2 AND NOT archived",&[&repository,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, String>(3) != "git" {
            return Err(PlatformError::Validation(
                "local snapshots do not support Git authorization",
            ));
        }
        let requested = row
            .get::<_, Option<String>>(0)
            .ok_or(PlatformError::Storage)?;
        let provider: GitProvider = PlatformStore::decode(Value::String(
            row.get::<_, Option<String>>(1)
                .ok_or(PlatformError::Storage)?,
        ))?;
        let registered_remote: Option<String> = row.get(2);
        let connection =
            GitConnectionStore::new(self.store).can_use(&ctx, &input.connection_id, project)?;
        if let (Some(registered), Some(expected)) = (&registered_remote, &input.expected_remote_id)
            && registered != expected
        {
            return Err(PlatformError::Conflict(
                "registered repository identity cannot be replaced",
            ));
        }
        let selected_remote = registered_remote
            .as_deref()
            .or(input.expected_remote_id.as_deref());
        if connection.provider != provider {
            return Err(PlatformError::Validation(
                "repository provider does not match connection",
            ));
        }
        let (remote_id, remote_name) = match provider {
            GitProvider::Github => crate::github_repository_proof::GithubRepositoryProof::verify(
                self.store,
                &ctx,
                &connection,
                &requested,
                selected_remote,
                policy,
            )?,
            GitProvider::Gitlab => {
                let tokens = match GitlabTokenStore::new(self.store).tokens(&connection, policy) {
                    Err(PlatformError::Unauthorized) => {
                        GitConnectionStore::new(self.store)
                            .record_access_failure(&ctx, &connection.id)?;
                        return Err(PlatformError::Unauthorized);
                    }
                    result => result?,
                };
                let client = GitlabClient::new(policy);
                match client.user(&tokens.access_token) {
                    Ok((id, _)) if id == connection.remote_account_id => {}
                    Err(PlatformError::Unauthorized) => {
                        GitConnectionStore::new(self.store)
                            .record_access_failure(&ctx, &connection.id)?;
                        return Err(PlatformError::Unauthorized);
                    }
                    Err(error) => return Err(error),
                    _ => return Err(PlatformError::Unauthorized),
                }
                let (remote_id, remote_name) = match client
                    .repository(&tokens.access_token, selected_remote.unwrap_or(&requested))
                {
                    Err(PlatformError::Unauthorized) => {
                        GitConnectionStore::new(self.store)
                            .record_access_failure(&ctx, &connection.id)?;
                        return Err(PlatformError::Unauthorized);
                    }
                    result => result?,
                };
                (remote_id, remote_name)
            }
        };
        if let Some(expected) = &input.expected_remote_id
            && expected != &remote_id
        {
            return Err(PlatformError::Conflict(
                "provider repository differs from selected identity",
            ));
        }
        let ctx = self.store.fresh(&ctx)?;
        ctx.require(Capability::RepositoriesWrite, Some(project))?;
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            cipher_request.as_ref(),
            ctx.actor_id(),
            "repository.authorize",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        tx.query_opt(
            "SELECT id FROM projects WHERE id=$1 AND NOT archived FOR SHARE",
            &[&project],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        let row=tx.query_opt("SELECT c.owner_actor_id,c.revision FROM git_connections c JOIN actors a ON a.id=c.owner_actor_id WHERE c.id=$1 AND c.status='active' AND a.active FOR SHARE OF c",&[&connection.id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        if row.get::<_, i64>(1) != connection.revision {
            return Err(PlatformError::Conflict(
                "connection authorization changed during provider verification",
            ));
        }
        let owner: String = row.get(0);
        if owner != ctx.actor_id() && ctx.require(Capability::InstallationManage, None).is_err() {
            tx.query_opt("SELECT connection_id FROM git_connection_projects WHERE connection_id=$1 AND project_id=$2 FOR SHARE",&[&connection.id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Forbidden)?;
        }
        let row=tx.query_opt("SELECT revision,remote_id,host FROM project_repositories WHERE id=$1 AND project_id=$2 AND NOT archived FOR UPDATE",&[&repository,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, i64>(0) != input.expected_revision {
            return Err(PlatformError::Conflict("repository revision changed"));
        }
        if let Some(previous) = row.get::<_, Option<String>>(1)
            && (previous != remote_id || row.get::<_, String>(2) != connection.host)
        {
            return Err(PlatformError::Conflict(
                "registered repository identity cannot be replaced",
            ));
        }
        let value:Value=tx.query_one("UPDATE project_repositories AS r SET remote_id=$2,remote_full_name=$3,host=$4,connection_id=$5,authorization_status='authorized',revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(r)",&[&repository,&remote_id,&remote_name,&connection.host,&connection.id]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "repository.provider_authorized",
            Some(repository),
            json!({"project_id":project,"connection_id":connection.id,"remote_id":remote_id}),
        )?;
        ReceiptWriter::save(&mut tx, cipher_request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
}
