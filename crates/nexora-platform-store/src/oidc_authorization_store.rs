use crate::{
    OidcNetworkPolicy, PlatformStore, ProviderSecretEnvelope, SecretGenerator,
    audit_writer::AuditWriter, oidc_client::OidcClient, oidc_pending_secrets::OidcPendingSecrets,
    oidc_settings_store::OidcSettingsStore,
};
use nexora_platform::{IssuedSession, OidcAuthorization, PlatformError};
use serde_json::json;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
pub(crate) struct OidcAuthorizationStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> OidcAuthorizationStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn start(
        &mut self,
        provider_id: &str,
        policy: &OidcNetworkPolicy,
    ) -> Result<OidcAuthorization, PlatformError> {
        let (provider, encrypted_secret) = OidcSettingsStore::new(self.store).load(provider_id)?;
        let cipher = self.store.provider_secret_cipher()?;
        let secret_context = OidcSettingsStore::context(
            &provider.id,
            &provider.issuer,
            "oidc_client_secret",
            provider.revision,
        )?;
        let secret = cipher.open(&secret_context, &encrypted_secret)?;
        let client = OidcClient::connect(
            &provider,
            std::str::from_utf8(&secret).map_err(|_| PlatformError::Storage)?,
            policy,
        )?;
        let id = SecretGenerator::id("oas")?;
        let state = format!("{id}.{}", SecretGenerator::token("state")?);
        let correlation = SecretGenerator::token("browser")?;
        let pending = OidcPendingSecrets {
            verifier: SecretGenerator::token("pkce")?,
            nonce: SecretGenerator::token("nonce")?,
        };
        let url = client.start_url(&state, &pending.nonce, &pending.verifier);
        let context = OidcSettingsStore::context(&id, &provider.issuer, "oidc_state", 1)?;
        let plain =
            Zeroizing::new(serde_json::to_vec(&pending).map_err(|_| PlatformError::Storage)?);
        let encrypted = serde_json::to_value(cipher.seal(&context, &plain)?)
            .map_err(|_| PlatformError::Storage)?;
        let written=self.store.client.execute("INSERT INTO oidc_authorizations(id,provider_id,provider_revision,state_digest,cookie_digest,pending_secrets,expires_at) SELECT $1,id,revision,$3,$4,$5,now()+interval '10 minutes' FROM oidc_providers WHERE id=$2 AND enabled AND revision=$6",&[&id,&provider.id,&SecretGenerator::digest(&state),&SecretGenerator::digest(&correlation),&encrypted,&provider.revision]).map_err(PlatformStore::database_error)?;
        if written != 1 {
            return Err(PlatformError::Unauthorized);
        }
        Ok(OidcAuthorization {
            authorization_url: url,
            state_id: id,
            correlation_secret: correlation,
            secure_cookie: provider.redirect_uri.starts_with("https://"),
        })
    }
    pub fn complete(
        &mut self,
        state: &str,
        code: &str,
        cookie: &str,
        policy: &OidcNetworkPolicy,
    ) -> Result<IssuedSession, PlatformError> {
        if state.len() > 256 || code.len() > 4096 || cookie.len() > 256 {
            return Err(PlatformError::Unauthorized);
        }
        let id = Self::state_id(state)?;
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let row=tx.query_opt("SELECT provider_id,provider_revision,state_digest,cookie_digest,pending_secrets,expires_at>now(),consumed_at IS NULL FROM oidc_authorizations WHERE id=$1 FOR UPDATE",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let state_hash: Vec<u8> = row.get(2);
        let cookie_hash: Vec<u8> = row.get(3);
        if !row.get::<_, bool>(5)
            || !row.get::<_, bool>(6)
            || !bool::from(state_hash.ct_eq(&SecretGenerator::digest(state)))
            || !bool::from(cookie_hash.ct_eq(&SecretGenerator::digest(cookie)))
        {
            return Err(PlatformError::Unauthorized);
        }
        let provider_id: String = row.get(0);
        let revision: i64 = row.get(1);
        let encrypted: ProviderSecretEnvelope = PlatformStore::decode(row.get(4))?;
        tx.execute(
            "UPDATE oidc_authorizations SET consumed_at=now() WHERE id=$1",
            &[&id],
        )
        .map_err(PlatformStore::database_error)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        if code.is_empty() {
            return Err(PlatformError::Unauthorized);
        }
        // Claim once before the external exchange; transport failure requires a new authorization flow.
        let (provider, encrypted_secret) = OidcSettingsStore::new(self.store).load(&provider_id)?;
        if revision != provider.revision {
            return Err(PlatformError::Unauthorized);
        }
        let cipher = self.store.provider_secret_cipher()?;
        let state_context = OidcSettingsStore::context(id, &provider.issuer, "oidc_state", 1)?;
        let plain = cipher.open(&state_context, &encrypted)?;
        let pending: OidcPendingSecrets =
            serde_json::from_slice(&plain).map_err(|_| PlatformError::Storage)?;
        let secret_context = OidcSettingsStore::context(
            &provider.id,
            &provider.issuer,
            "oidc_client_secret",
            provider.revision,
        )?;
        let secret = cipher.open(&secret_context, &encrypted_secret)?;
        let client = OidcClient::connect(
            &provider,
            std::str::from_utf8(&secret).map_err(|_| PlatformError::Storage)?,
            policy,
        )?;
        let identity = client.exchange_subject(code, &pending.verifier, &pending.nonce)?;
        let subject = identity.subject;
        let session_id = SecretGenerator::id("ses")?;
        let token = format!("nxs_{session_id}.{}", SecretGenerator::token("secret")?);
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let current = tx
            .query_opt(
                "SELECT revision FROM oidc_providers WHERE id=$1 AND enabled FOR SHARE",
                &[&provider.id],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Unauthorized)?;
        if current.get::<_, i64>(0) != revision {
            return Err(PlatformError::Unauthorized);
        }
        let existing=tx.query_opt("SELECT actor_id FROM oidc_identities WHERE provider_id=$1 AND issuer=$2 AND subject=$3",&[&provider.id,&provider.issuer,&subject]).map_err(PlatformStore::database_error)?;
        if existing.is_none() {
            // Serialize first logins for the same provider/subject without trusting email aliases.
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
                &[&format!("{}:{}", provider.id, subject)],
            )
            .map_err(PlatformStore::database_error)?;
            let found = tx
                .query_opt(
                    "SELECT actor_id FROM oidc_identities WHERE provider_id=$1 AND subject=$2",
                    &[&provider.id, &subject],
                )
                .map_err(PlatformStore::database_error)?;
            if found.is_none() {
                let actor = SecretGenerator::id("act")?;
                let username = format!("oidc_{}", &actor[4..]);
                tx.execute("INSERT INTO actors(id,kind,name,purpose) VALUES($1,'human',$2,'Pending corporate access')",&[&actor,&identity.name.clone().unwrap_or_else(||username.clone())]).map_err(PlatformStore::database_error)?;
                tx.execute(
                    "INSERT INTO users(actor_id,username) VALUES($1,$2)",
                    &[&actor, &username],
                )
                .map_err(PlatformStore::database_error)?;
                tx.execute("INSERT INTO oidc_identities(issuer,subject,provider_id,actor_id) VALUES($1,$2,$3,$4)",&[&provider.issuer,&subject,&provider.id,&actor]).map_err(PlatformStore::database_error)?;

                tx.execute(
                    "INSERT INTO user_security(actor_id,pending_role) VALUES($1,true)",
                    &[&actor],
                )
                .map_err(PlatformStore::database_error)?;
                AuditWriter::append(
                    &mut tx,
                    Some(&actor),
                    None,
                    "identity.access_requested",
                    Some(&actor),
                    json!({"provider_id":provider.id}),
                )?;
            }
        }
        let actor:Option<String>=tx.query_opt("INSERT INTO sessions(id,actor_id,digest,expires_at,authentication_provider_id) SELECT $1,a.id,$2,clock_timestamp()+interval '30 minutes',p.id FROM oidc_providers p JOIN oidc_identities i ON i.provider_id=p.id AND i.issuer=p.issuer JOIN users u ON u.actor_id=i.actor_id JOIN actors a ON a.id=u.actor_id WHERE p.id=$3 AND p.enabled AND p.revision=$4 AND i.subject=$5 AND p.issuer=$6 AND a.active AND a.kind='human' RETURNING actor_id",&[&session_id,&SecretGenerator::digest(&token),&provider.id,&revision,&subject,&provider.issuer]).map_err(PlatformStore::database_error)?.map(|r|r.get(0));
        let actor = actor.ok_or(PlatformError::Unauthorized)?;
        if let Some(email) = &identity.email {
            tx.execute("UPDATE oidc_identities SET contact_email=$3,contact_email_verified=$4 WHERE provider_id=$1 AND actor_id=$2 AND subject=$5",&[&provider.id,&actor,email,&identity.email_verified,&subject]).map_err(PlatformStore::database_error)?;
            if identity.email_verified {
                tx.execute("INSERT INTO notification_recipients(actor_id,email) VALUES($1,$2) ON CONFLICT(actor_id) DO UPDATE SET email=EXCLUDED.email,revision=notification_recipients.revision+1 WHERE notification_recipients.email=''",&[&actor,email]).map_err(PlatformStore::database_error)?;
            }
        }
        AuditWriter::append(
            &mut tx,
            Some(&actor),
            Some(&session_id),
            "session.corporate_created",
            Some(&session_id),
            json!({"provider_id":provider.id}),
        )?;
        let next_step = crate::login_gate::LoginGate::apply(&mut tx, &actor, &session_id)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(IssuedSession {
            access_token: token,
            token_type: "Bearer".into(),
            expires_in: 28800,
            actor_id: actor,
            next_step,
        })
    }
    pub fn state_id(state: &str) -> Result<&str, PlatformError> {
        let (id, secret) = state.split_once('.').ok_or(PlatformError::Unauthorized)?;
        if id.len() != 36
            || !id.starts_with("oas_")
            || !id[4..].bytes().all(|b| b.is_ascii_hexdigit())
            || secret.is_empty()
        {
            return Err(PlatformError::Unauthorized);
        }
        Ok(id)
    }
}
