use crate::{
    GitNetworkPolicy, PlatformStore, ProviderSecretEnvelope, SecretGenerator, WebhookProof,
    audit_writer::AuditWriter, gitlab_settings_store::GitlabSettingsStore,
    receipt_writer::ReceiptWriter, source_event_writer::SourceEventWriter,
    webhook_delivery_writer::WebhookDeliveryWriter, webhook_signature::WebhookSignature,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct GitlabWebhookStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> GitlabWebhookStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn register(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        input: &GitlabWebhookRegistration,
        policy: &GitNetworkPolicy,
    ) -> Result<GitlabWebhook, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::RepositoriesWrite, Some(project))?;
        let rawkey = WebhookSignature::key(&input.signing_token)?;
        let cipher = self.store.provider_secret_cipher()?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "gitlab.webhook.register",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let row=tx.query_opt("SELECT r.revision,r.host,r.remote_id FROM project_repositories r JOIN projects p ON p.id=r.project_id JOIN git_connections c ON c.id=r.connection_id JOIN actors a ON a.id=c.owner_actor_id WHERE r.id=$1 AND r.project_id=$2 AND r.provider='gitlab' AND NOT r.archived AND NOT p.archived AND r.authorization_status='authorized' AND c.status='active' AND c.provider='gitlab' AND a.active FOR UPDATE OF r",&[&repository,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, i64>(0) != input.expected_revision {
            return Err(PlatformError::Conflict("repository revision changed"));
        }
        let host: String = row.get(1);
        let remote: String = row.get(2);
        if host != policy.host()? {
            return Err(PlatformError::Conflict("GitLab operator host changed"));
        }
        let count: i64 = tx
            .query_one(
                "SELECT count(*) FROM gitlab_webhooks WHERE repository_id=$1 AND active",
                &[&repository],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        if count >= 100 {
            return Err(PlatformError::Validation(
                "repository webhook limit reached",
            ));
        }
        let id = SecretGenerator::id("whk")?;
        let callback = format!("{}/api/v1/git/gitlab/webhooks/{id}", policy.callback_origin);
        let encrypted = serde_json::to_value(cipher.seal(
            &GitlabSettingsStore::context(&id, &host, "gitlab_webhook_signing", 1),
            &rawkey,
        )?)
        .map_err(|_| PlatformError::Storage)?;
        let value:Value=tx.query_one("INSERT INTO gitlab_webhooks AS h(id,repository_id,registered_by,host,remote_id,signing_key,callback_url) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING to_jsonb(h)-'signing_key'",&[&id,&repository,&ctx.actor_id(),&host,&remote,&encrypted,&callback]).map_err(PlatformStore::database_error)?.get(0);
        let metadata: GitlabWebhook = PlatformStore::decode(value)?;
        let value = serde_json::to_value(&metadata).map_err(|_| PlatformError::Storage)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.webhook_registered",
            Some(&id),
            json!({"repository_id":repository,"provider":"gitlab"}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(metadata)
    }
    pub fn list(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<Vec<GitlabWebhook>, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::RepositoriesRead, Some(project))?;
        self.store.get_repository(&ctx, project, repository)?;
        self.store.client.query("SELECT to_jsonb(h)-'signing_key' FROM gitlab_webhooks h WHERE repository_id=$1 AND active ORDER BY id LIMIT 100",&[&repository]).map_err(PlatformStore::database_error)?.iter().map(|r|PlatformStore::decode(r.get(0))).collect()
    }
    pub fn revoke(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        id: &str,
    ) -> Result<(), PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::RepositoriesWrite, Some(project))?;
        self.store.get_repository(&ctx, project, repository)?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "gitlab.webhook.revoke",
        )? {
            let _: () = PlatformStore::decode(v)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(());
        }
        let affected=tx.execute("UPDATE gitlab_webhooks SET active=false,revision=revision+1 WHERE id=$1 AND repository_id=$2 AND active",&[&id,&repository]).map_err(PlatformStore::database_error)?;
        if affected == 0
            && !tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM gitlab_webhooks WHERE id=$1 AND repository_id=$2)",
                    &[&id, &repository],
                )
                .map_err(PlatformStore::database_error)?
                .get::<_, bool>(0)
        {
            return Err(PlatformError::NotFound);
        }
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.webhook_revoked",
            Some(id),
            json!({"repository_id":repository,"provider":"gitlab"}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &Value::Null)?;
        tx.commit().map_err(PlatformStore::database_error)
    }
    pub fn receive(
        &mut self,
        id: &str,
        proof: &WebhookProof,
        body: &[u8],
        policy: &GitNetworkPolicy,
    ) -> Result<WebhookReceipt, PlatformError> {
        proof.validate()?;
        let row=self.store.client.query_opt("SELECT signing_key,host,remote_id,repository_id,revision FROM gitlab_webhooks WHERE id=$1 AND active",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let host: String = row.get(1);
        let remote: String = row.get(2);
        let repository: String = row.get(3);
        let revision: i64 = row.get(4);
        if host != policy.host()? {
            return Err(PlatformError::Unauthorized);
        }
        let envelope: ProviderSecretEnvelope = PlatformStore::decode(row.get(0))?;
        // Secret encryption stays at credential revision1; active/revoked metadata revision is separate.
        let key = self.store.provider_secret_cipher()?.open(
            &GitlabSettingsStore::context(id, &host, "gitlab_webhook_signing", 1),
            &envelope,
        )?;
        WebhookSignature::standard(&key, proof, body)?;
        let value: Value = serde_json::from_slice(body)
            .map_err(|_| PlatformError::Validation("invalid webhook JSON"))?;
        let payload_remote = value
            .get("project_id")
            .or_else(|| value.get("project").and_then(|v| v.get("id")))
            .ok_or(PlatformError::Validation("missing webhook project"))?;
        if payload_remote
            .as_u64()
            .filter(|v| *v > 0)
            .map(|v| v.to_string())
            .ok_or(PlatformError::Validation("invalid webhook project id"))?
            != remote
        {
            return Err(PlatformError::Forbidden);
        }
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if WebhookDeliveryWriter::duplicate(
            &mut tx,
            &self.store.namespace,
            "gitlab",
            id,
            proof,
            body,
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(WebhookReceipt { duplicate: true });
        }
        tx.query_opt(
            "SELECT id FROM gitlab_webhooks WHERE id=$1 AND active AND revision=$2 FOR SHARE",
            &[&id, &revision],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::Unauthorized)?;
        let row=tx.query_opt("SELECT r.id FROM project_repositories r JOIN projects p ON p.id=r.project_id JOIN git_connections c ON c.id=r.connection_id JOIN actors a ON a.id=c.owner_actor_id WHERE r.id=$1 AND r.host=$2 AND r.remote_id=$3 AND r.provider='gitlab' AND NOT r.archived AND NOT p.archived AND r.authorization_status='authorized' AND c.status='active' AND c.provider='gitlab' AND a.active FOR UPDATE OF r",&[&repository,&host,&remote]).map_err(PlatformStore::database_error)?;
        if row.is_some() {
            match (proof.event.as_str(), value["object_kind"].as_str()) {
                ("Push Hook", Some("push")) | ("Tag Push Hook", Some("tag_push")) => {
                    crate::git_author_observation_writer::GitAuthorObservationWriter::record(
                        &mut tx,
                        "gitlab",
                        &host,
                        id,
                        &proof.delivery_id,
                        &repository,
                        &value,
                    )?;
                    crate::developer_push_writer::DeveloperPushWriter::record(
                        &mut tx,
                        "gitlab",
                        &host,
                        id,
                        &proof.delivery_id,
                        &repository,
                        &value,
                    )?;
                    SourceEventWriter::changed(
                        &mut tx,
                        &repository,
                        "gitlab",
                        id,
                        proof,
                        SourceEventWriter::source(&value)?,
                    )?
                }
                _ => {}
            }
        }
        WebhookDeliveryWriter::save(&mut tx, "gitlab", id, proof, body)?;
        AuditWriter::append(
            &mut tx,
            None,
            None,
            "git.webhook_received",
            Some(&proof.delivery_id),
            json!({"provider":"gitlab","event":proof.event,"repository_id":repository}),
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(WebhookReceipt { duplicate: false })
    }
}
