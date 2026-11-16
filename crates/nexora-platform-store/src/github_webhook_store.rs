use crate::{
    GitNetworkPolicy, PlatformStore, WebhookProof, audit_writer::AuditWriter,
    github_client::GithubClient, github_settings_store::GithubSettingsStore,
    source_event_writer::SourceEventWriter, webhook_delivery_writer::WebhookDeliveryWriter,
    webhook_signature::WebhookSignature,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct GithubWebhookStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> GithubWebhookStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn receive(
        &mut self,
        proof: &WebhookProof,
        body: &[u8],
        policy: &GitNetworkPolicy,
    ) -> Result<WebhookReceipt, PlatformError> {
        let (config, secrets) = GithubSettingsStore::new(self.store).load(policy)?;
        WebhookSignature::github(&secrets.webhook_secret, proof, body)?;
        let value: Value = serde_json::from_slice(body)
            .map_err(|_| PlatformError::Validation("invalid webhook JSON"))?;
        if !value.is_object() {
            return Err(PlatformError::Validation("expected webhook object"));
        }
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if WebhookDeliveryWriter::duplicate(
            &mut tx,
            &self.store.namespace,
            "github",
            &config.app_id,
            proof,
            body,
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(WebhookReceipt { duplicate: true });
        }
        tx.query_opt("SELECT revision FROM github_settings WHERE singleton AND enabled AND revision=$1 FOR SHARE",&[&config.revision]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let action = value["action"].as_str().unwrap_or("");
        match (proof.event.as_str(), action) {
            ("push", _) => {
                let installation = GithubClient::id(&value["installation"]["id"])?;
                let remote = GithubClient::id(&value["repository"]["id"])?;
                let source = SourceEventWriter::source(&value)?;
                let rows=tx.query("SELECT r.id FROM project_repositories r JOIN git_connections c ON c.id=r.connection_id JOIN actors a ON a.id=c.owner_actor_id JOIN projects p ON p.id=r.project_id WHERE r.provider='github' AND r.host=$1 AND r.remote_id=$2 AND c.installation_id=$3 AND c.status='active' AND a.active AND NOT p.archived AND NOT r.archived AND r.authorization_status='authorized' FOR UPDATE OF r",&[&config.host,&remote,&installation]).map_err(PlatformStore::database_error)?;
                for row in rows {
                    crate::git_author_observation_writer::GitAuthorObservationWriter::record(
                        &mut tx,
                        "github",
                        &config.host,
                        &config.app_id,
                        &proof.delivery_id,
                        &row.get::<_, String>(0),
                        &value,
                    )?;
                    crate::developer_push_writer::DeveloperPushWriter::record(
                        &mut tx,
                        "github",
                        &config.host,
                        &config.app_id,
                        &proof.delivery_id,
                        &row.get::<_, String>(0),
                        &value,
                    )?;
                    SourceEventWriter::changed(
                        &mut tx,
                        &row.get::<_, String>(0),
                        "github",
                        &config.app_id,
                        proof,
                        source,
                    )?;
                }
            }
            ("installation", "deleted" | "suspend") => {
                if GithubClient::id(&value["installation"]["app_id"])? != config.app_id {
                    return Err(PlatformError::Forbidden);
                }
                let installation = GithubClient::id(&value["installation"]["id"])?;
                tx.execute("UPDATE git_connections SET status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE provider='github' AND host=$1 AND installation_id=$2 AND status='active'",&[&config.host,&installation]).map_err(PlatformStore::database_error)?;
                tx.execute("UPDATE project_repositories SET authorization_status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE authorization_status='authorized' AND connection_id IN (SELECT id FROM git_connections WHERE provider='github' AND host=$1 AND installation_id=$2)",&[&config.host,&installation]).map_err(PlatformStore::database_error)?;
            }
            ("github_app_authorization", "revoked") => {
                let account = GithubClient::id(&value["sender"]["id"])?;
                tx.execute("UPDATE git_connections SET status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE provider='github' AND host=$1 AND remote_account_id=$2 AND status='active'",&[&config.host,&account]).map_err(PlatformStore::database_error)?;
                tx.execute("UPDATE project_repositories SET authorization_status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE authorization_status='authorized' AND connection_id IN (SELECT id FROM git_connections WHERE provider='github' AND host=$1 AND remote_account_id=$2)",&[&config.host,&account]).map_err(PlatformStore::database_error)?;
            }
            ("installation_repositories", "removed") => {
                if GithubClient::id(&value["installation"]["app_id"])? != config.app_id {
                    return Err(PlatformError::Forbidden);
                }
                let installation = GithubClient::id(&value["installation"]["id"])?;
                let removed = value["repositories_removed"]
                    .as_array()
                    .filter(|v| v.len() <= 10_000)
                    .ok_or(PlatformError::Validation("invalid removed repository list"))?;
                tx.execute("UPDATE git_connections SET revision=revision+1,updated_at=clock_timestamp() WHERE provider='github' AND host=$1 AND installation_id=$2 AND status='active'",&[&config.host,&installation]).map_err(PlatformStore::database_error)?;
                let ids: Vec<String> = removed
                    .iter()
                    .map(|v| GithubClient::id(&v["id"]))
                    .collect::<Result<_, _>>()?;
                // GitHub can omit removed IDs on all -> selected. Require explicit revalidation then.
                tx.execute("UPDATE project_repositories SET authorization_status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE authorization_status='authorized' AND (cardinality($3::text[])=0 OR remote_id=ANY($3)) AND connection_id IN (SELECT id FROM git_connections WHERE provider='github' AND host=$1 AND installation_id=$2)",&[&config.host,&installation,&ids]).map_err(PlatformStore::database_error)?;
            }
            ("repository", "deleted") => {
                let installation = GithubClient::id(&value["installation"]["id"])?;
                let remote = GithubClient::id(&value["repository"]["id"])?;
                tx.execute("UPDATE project_repositories SET authorization_status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE remote_id=$1 AND authorization_status='authorized' AND connection_id IN (SELECT id FROM git_connections WHERE provider='github' AND host=$2 AND installation_id=$3)",&[&remote,&config.host,&installation]).map_err(PlatformStore::database_error)?;
            }
            _ => {}
        }
        WebhookDeliveryWriter::save(&mut tx, "github", &config.app_id, proof, body)?;
        AuditWriter::append(
            &mut tx,
            None,
            None,
            "git.webhook_received",
            Some(&proof.delivery_id),
            json!({"provider":"github","event":proof.event,"scope_id":config.app_id}),
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(WebhookReceipt { duplicate: false })
    }
}
