use crate::{
    OidcNetworkPolicy, PlatformStore, ProviderSecretContext, ProviderSecretEnvelope,
    SecretAuthority, SecretGenerator, audit_writer::AuditWriter, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct OidcSettingsStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> OidcSettingsStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn change(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        change: &OidcProviderChange,
        policy: &OidcNetworkPolicy,
    ) -> Result<OidcProvider, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
        let request = self.store.request_context.clone();
        // Check the receipt before reading the newer revision so uncertain PATCH retries replay safely.
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
            "oidc.change",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        let row = self.store.client.query_opt("SELECT to_jsonb(p)-'client_secret',client_secret FROM oidc_providers p WHERE id=$1", &[&id])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let current: OidcProvider = PlatformStore::decode(row.get(0))?;
        if current.revision != change.expected_revision {
            return Err(PlatformError::Conflict("OIDC provider revision changed"));
        }
        if change.client_secret.is_none()
            && (change
                .client_id
                .as_ref()
                .is_some_and(|id| *id != current.client_id)
                || change
                    .redirect_uri
                    .as_ref()
                    .is_some_and(|uri| *uri != current.redirect_uri))
        {
            return Err(PlatformError::Validation(
                "OIDC client or callback change requires a new client secret",
            ));
        }
        let secret = if let Some(value) = &change.client_secret {
            value.clone()
        } else {
            let envelope: ProviderSecretEnvelope = PlatformStore::decode(row.get(1))?;
            let plain = self.store.provider_secret_cipher()?.open(
                &Self::context(
                    &current.id,
                    &current.issuer,
                    "oidc_client_secret",
                    current.revision,
                )?,
                &envelope,
            )?;
            String::from_utf8(plain.to_vec()).map_err(|_| PlatformError::Storage)?
        };
        let input = OidcProviderRegistration {
            name: change.name.clone().unwrap_or(current.name),
            issuer: current.issuer,
            client_id: change.client_id.clone().unwrap_or(current.client_id),
            client_secret: secret,
            redirect_uri: change.redirect_uri.clone().unwrap_or(current.redirect_uri),
            enabled: change.enabled.unwrap_or(current.enabled),
            expected_revision: current.revision,
        };
        self.configure_operation(&ctx, &input, policy, "oidc.change")
    }
    pub fn configure(
        &mut self,
        ctx: &AuthContext,
        input: &OidcProviderRegistration,
        policy: &OidcNetworkPolicy,
    ) -> Result<OidcProvider, PlatformError> {
        self.configure_operation(ctx, input, policy, "oidc.configure")
    }
    fn configure_operation(
        &mut self,
        ctx: &AuthContext,
        input: &OidcProviderRegistration,
        policy: &OidcNetworkPolicy,
        operation: &str,
    ) -> Result<OidcProvider, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
        policy.registration(input)?;
        let name = DomainText::name(&input.name)?;
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
            operation,
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let local: bool = tx
            .query_one(
                "SELECT local_enabled FROM authentication_policy WHERE singleton FOR UPDATE",
                &[],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        let previous = tx
            .query_opt(
                "SELECT id,revision FROM oidc_providers WHERE issuer=$1 FOR UPDATE",
                &[&input.issuer],
            )
            .map_err(PlatformStore::database_error)?;
        let (id, revision) = match previous {
            Some(row) => {
                let rev: i64 = row.get(1);
                if rev != input.expected_revision {
                    return Err(PlatformError::Conflict("OIDC provider revision changed"));
                }
                (row.get::<_, String>(0), rev + 1)
            }
            None => {
                if input.expected_revision != 0 {
                    return Err(PlatformError::Conflict(
                        "OIDC provider does not exist at expected revision",
                    ));
                }
                (SecretGenerator::id("idp")?, 1)
            }
        };
        if !local && !input.enabled {
            let available:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM oidc_providers p JOIN oidc_identities i ON i.provider_id=p.id JOIN actors a ON a.id=i.actor_id JOIN actor_grants g ON g.actor_id=a.id WHERE p.enabled AND p.id<>$1 AND a.active AND g.capability='installation:manage' AND g.project_id IS NULL)",&[&id]).map_err(PlatformStore::database_error)?.get(0);
            if !available {
                return Err(PlatformError::Conflict(
                    "cannot disable the last corporate administrator login",
                ));
            }
        }
        let context = Self::context(&id, &input.issuer, "oidc_client_secret", revision)?;
        let secret = serde_json::to_value(cipher.seal(&context, input.client_secret.as_bytes())?)
            .map_err(|_| PlatformError::Storage)?;
        let value:Value=tx.query_one("INSERT INTO oidc_providers AS p(id,name,issuer,client_id,client_secret,redirect_uri,enabled,revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(issuer) DO UPDATE SET name=EXCLUDED.name,client_id=EXCLUDED.client_id,client_secret=EXCLUDED.client_secret,redirect_uri=EXCLUDED.redirect_uri,enabled=EXCLUDED.enabled,revision=EXCLUDED.revision,updated_at=clock_timestamp() RETURNING to_jsonb(p)-'client_secret'",&[&id,&name,&input.issuer,&input.client_id,&secret,&input.redirect_uri,&input.enabled,&revision]).map_err(PlatformStore::database_error)?.get(0);
        if !input.enabled {
            tx.execute("UPDATE sessions SET revoked_at=now() WHERE authentication_provider_id=$1 AND revoked_at IS NULL",&[&id]).map_err(PlatformStore::database_error)?;
        }
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "identity.provider_configured",
            Some(&id),
            json!({"revision":revision,"enabled":input.enabled}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
    pub fn list(&mut self, ctx: &AuthContext) -> Result<Vec<OidcProvider>, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
        self.store
            .client
            .query(
                "SELECT to_jsonb(p)-'client_secret' FROM oidc_providers p ORDER BY id LIMIT 100",
                &[],
            )
            .map_err(PlatformStore::database_error)?
            .iter()
            .map(|r| PlatformStore::decode(r.get(0)))
            .collect()
    }
    pub fn load(
        &mut self,
        id: &str,
    ) -> Result<(OidcProvider, ProviderSecretEnvelope), PlatformError> {
        let row=self.store.client.query_opt("SELECT to_jsonb(p)-'client_secret',client_secret FROM oidc_providers p WHERE id=$1 AND enabled",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        Ok((
            PlatformStore::decode(row.get(0))?,
            PlatformStore::decode(row.get(1))?,
        ))
    }
    pub fn context(
        id: &str,
        issuer: &str,
        purpose: &str,
        revision: i64,
    ) -> Result<ProviderSecretContext, PlatformError> {
        let url =
            url::Url::parse(issuer).map_err(|_| PlatformError::Validation("invalid issuer"))?;
        Ok(ProviderSecretContext {
            binding_id: id.into(),
            provider: SecretAuthority::OpenIdConnect,
            host: url
                .host_str()
                .ok_or(PlatformError::Validation("issuer host required"))?
                .into(),
            purpose: purpose.into(),
            revision,
        })
    }
    pub fn authentication_policy(&mut self) -> Result<AuthenticationPolicy, PlatformError> {
        let row = self
            .store
            .client
            .query_one(
                "SELECT local_enabled,revision FROM authentication_policy WHERE singleton",
                &[],
            )
            .map_err(PlatformStore::database_error)?;
        Ok(AuthenticationPolicy {
            local_enabled: row.get(0),
            revision: row.get(1),
        })
    }
    pub fn change_policy(
        &mut self,
        ctx: &AuthContext,
        input: &AuthenticationPolicyChange,
    ) -> Result<AuthenticationPolicy, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
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
            "auth.policy",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let row = tx
            .query_one(
                "SELECT revision FROM authentication_policy WHERE singleton FOR UPDATE",
                &[],
            )
            .map_err(PlatformStore::database_error)?;
        if row.get::<_, i64>(0) != input.expected_revision {
            return Err(PlatformError::Conflict(
                "authentication policy revision changed",
            ));
        }
        if !input.local_enabled {
            let available:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM oidc_providers p JOIN oidc_identities i ON i.provider_id=p.id JOIN actors a ON a.id=i.actor_id JOIN actor_grants g ON g.actor_id=a.id WHERE p.enabled AND a.active AND g.capability='installation:manage' AND g.project_id IS NULL AND EXISTS(SELECT 1 FROM sessions s WHERE s.actor_id=a.id AND s.authentication_provider_id=p.id AND s.access_stage='ready' AND s.revoked_at IS NULL AND s.expires_at>now()))",&[]).map_err(PlatformStore::database_error)?.get(0);
            if !available {
                return Err(PlatformError::Conflict(
                    "verify a corporate administrator login before disabling local login",
                ));
            }
            tx.execute("UPDATE sessions SET revoked_at=now() WHERE authentication_provider_id IS NULL AND revoked_at IS NULL",&[]).map_err(PlatformStore::database_error)?;
        }
        let value:Value=tx.query_one("UPDATE authentication_policy AS p SET local_enabled=$1,revision=revision+1 WHERE singleton RETURNING jsonb_build_object('local_enabled',p.local_enabled,'revision',p.revision)",&[&input.local_enabled]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "identity.policy_changed",
            None,
            json!({"local_enabled":input.local_enabled,"revision":value["revision"]}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
}
