use crate::receipt_writer::ReceiptWriter;
use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    password_credential::PasswordCredential,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct IdentityStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> IdentityStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn consume_authentication_attempt(
        &mut self,
        key: &str,
        limit: i64,
        window_seconds: i64,
    ) -> Result<(), PlatformError> {
        if key.is_empty() || key.len() > 512 || limit <= 0 || window_seconds <= 0 {
            return Err(PlatformError::Validation("invalid authentication limit"));
        }
        self.store
            .client
            .execute(
                "DELETE FROM authentication_attempt_limits WHERE window_started_at<clock_timestamp()-interval '1 day'",
                &[],
            )
            .map_err(PlatformStore::database_error)?;
        let digest = SecretGenerator::digest(&format!("authentication-attempt:{key}"));
        let attempts: i64 = self
            .store
            .client
            .query_one(
                "INSERT INTO authentication_attempt_limits(key_digest,window_started_at,attempts) VALUES($1,clock_timestamp(),1) ON CONFLICT(key_digest) DO UPDATE SET window_started_at=CASE WHEN authentication_attempt_limits.window_started_at<=clock_timestamp()-($2*interval '1 second') THEN clock_timestamp() ELSE authentication_attempt_limits.window_started_at END,attempts=CASE WHEN authentication_attempt_limits.window_started_at<=clock_timestamp()-($2*interval '1 second') THEN 1 ELSE authentication_attempt_limits.attempts+1 END RETURNING attempts",
                &[&digest, &window_seconds],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        if attempts > limit {
            return Err(PlatformError::TooManyRequests);
        }
        Ok(())
    }

    fn clear_authentication_attempt(&mut self, key: &str) -> Result<(), PlatformError> {
        let digest = SecretGenerator::digest(&format!("authentication-attempt:{key}"));
        self.store
            .client
            .execute(
                "DELETE FROM authentication_attempt_limits WHERE key_digest=$1",
                &[&digest],
            )
            .map_err(PlatformStore::database_error)?;
        Ok(())
    }
    pub fn bootstrap(
        &mut self,
        organization: &str,
        username: &str,
        password: &str,
        name: &str,
    ) -> Result<BootstrapResult, PlatformError> {
        let organization = DomainText::name(organization)?;
        let username = DomainText::username(username)?;
        let owner = Actor::new(
            SecretGenerator::id("act")?,
            ActorKind::Human,
            name,
            "Installation bootstrap",
        )?;
        let hash = PasswordCredential::hash(password)?;
        let installation = SecretGenerator::id("ins")?;
        let org = SecretGenerator::id("org")?;
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!(
                "nexora-platform-bootstrap:{}",
                self.store.namespace
            )],
        )
        .map_err(PlatformStore::database_error)?;
        if tx
            .query_opt("SELECT id FROM installation WHERE singleton", &[])
            .map_err(PlatformStore::database_error)?
            .is_some()
        {
            return Err(PlatformError::Conflict("installation already initialized"));
        }
        tx.execute(
            "INSERT INTO installation(id,organization_id,organization_name) VALUES($1,$2,$3)",
            &[&installation, &org, &organization],
        )
        .map_err(PlatformStore::database_error)?;
        tx.execute(
            "UPDATE organization_profile SET name=$1 WHERE singleton AND name=''",
            &[&organization],
        )
        .map_err(PlatformStore::database_error)?;
        tx.execute(
            "INSERT INTO actors(id,kind,name,purpose) VALUES($1,'human',$2,$3)",
            &[&owner.id, &owner.name, &owner.purpose],
        )
        .map_err(PlatformStore::database_error)?;
        tx.execute(
            "INSERT INTO users(actor_id,username,password_hash) VALUES($1,$2,$3)",
            &[&owner.id, &username, &hash],
        )
        .map_err(PlatformStore::database_error)?;
        tx.execute(
            "INSERT INTO actor_roles(actor_id,role,all_projects) VALUES($1,'super_admin',true)",
            &[&owner.id],
        )
        .map_err(PlatformStore::database_error)?;
        for capability in Capability::all() {
            tx.execute(
                "INSERT INTO actor_grants(actor_id,capability) VALUES($1,$2)",
                &[&owner.id, &capability.as_str()],
            )
            .map_err(PlatformStore::database_error)?;
        }
        AuditWriter::append(
            &mut tx,
            Some(&owner.id),
            None,
            "installation.bootstrap",
            Some(&installation),
            json!({"organization_id":org}),
        )?;
        let owner = PlatformStore::decode(
            tx.query_one("SELECT to_jsonb(a) FROM actors a WHERE id=$1", &[&owner.id])
                .map_err(PlatformStore::database_error)?
                .get(0),
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(BootstrapResult {
            installation_id: installation,
            organization_id: org,
            organization_name: organization,
            owner,
        })
    }
    pub fn login(
        &mut self,
        username: &str,
        password: &str,
    ) -> Result<IssuedSession, PlatformError> {
        self.login_from_peer(username, password, None)
    }

    pub fn login_from_peer(
        &mut self,
        username: &str,
        password: &str,
        peer: Option<&str>,
    ) -> Result<IssuedSession, PlatformError> {
        if !self.store.authentication_policy()?.local_enabled {
            return Err(PlatformError::Unauthorized);
        }
        let username = DomainText::username(username).map_err(|_| PlatformError::Unauthorized)?;
        self.consume_authentication_attempt(&format!("login:account:{username}"), 10, 900)?;
        if let Some(peer) = peer {
            self.consume_authentication_attempt(&format!("login:peer:{peer}"), 60, 900)?;
        }
        if password.len() > 1024 {
            return Err(PlatformError::Unauthorized);
        }
        let row=self.store.client.query_opt("SELECT a.id,u.password_hash FROM users u JOIN actors a ON a.id=u.actor_id WHERE u.username=$1 AND a.active AND a.kind='human'",&[&username]).map_err(PlatformStore::database_error)?;
        let Some(row) = row else {
            let _ = PasswordCredential::hash(&SecretGenerator::token("dummy")?)?;
            return Err(PlatformError::Unauthorized);
        };
        let hash: Option<String> = row.get(1);
        if hash
            .as_ref()
            .is_none_or(|h| !PasswordCredential::verify(password, h))
        {
            if hash.is_none() {
                let _ = PasswordCredential::hash(&SecretGenerator::token("dummy")?)?;
            }
            return Err(PlatformError::Unauthorized);
        }
        let upgraded_hash = hash
            .as_deref()
            .filter(|stored| PasswordCredential::needs_rehash(stored))
            .map(|_| PasswordCredential::hash(password))
            .transpose()?;
        let actor: String = row.get(0);
        let id = SecretGenerator::id("ses")?;
        let token = format!("nxs_{id}.{}", SecretGenerator::token("secret")?);
        let digest = SecretGenerator::digest(&token);
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let local_enabled: bool = tx
            .query_one(
                "SELECT local_enabled FROM authentication_policy WHERE singleton FOR SHARE",
                &[],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        if !local_enabled {
            return Err(PlatformError::Unauthorized);
        }
        // Bind the pre-transaction password verification to the current credential.
        // Recovery locks these rows before changing the hash and revoking sessions;
        // retaining shared locks until commit orders session creation with recovery.
        let current = tx
            .query_opt(
                "SELECT u.password_hash FROM actors a JOIN users u ON u.actor_id=a.id WHERE a.id=$1 AND a.active AND a.kind='human' FOR SHARE OF a,u",
                &[&actor],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Unauthorized)?;
        if current.get::<_, Option<String>>(0) != hash {
            return Err(PlatformError::Unauthorized);
        }
        if let Some(upgraded_hash) = upgraded_hash {
            tx.execute(
                "UPDATE users SET password_hash=$2 WHERE actor_id=$1",
                &[&actor, &upgraded_hash],
            )
            .map_err(PlatformStore::database_error)?;
        }
        let affected=tx.execute("INSERT INTO sessions(id,actor_id,digest,expires_at) SELECT $1,id,$3,clock_timestamp()+interval '30 minutes' FROM actors WHERE id=$2 AND active AND kind='human' AND EXISTS(SELECT 1 FROM authentication_policy WHERE singleton AND local_enabled)",&[&id,&actor,&digest]).map_err(PlatformStore::database_error)?;
        if affected != 1 {
            return Err(PlatformError::Unauthorized);
        }
        AuditWriter::append(
            &mut tx,
            Some(&actor),
            Some(&id),
            "session.created",
            Some(&id),
            json!({}),
        )?;
        let next_step = crate::login_gate::LoginGate::apply(&mut tx, &actor, &id)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        self.clear_authentication_attempt(&format!("login:account:{username}"))?;
        if let Some(peer) = peer {
            self.clear_authentication_attempt(&format!("login:peer:{peer}"))?;
        }
        Ok(IssuedSession {
            access_token: token,
            token_type: "Bearer".into(),
            expires_in: 28800,
            actor_id: actor,
            next_step,
        })
    }
    pub fn logout(&mut self, token: &str) -> Result<(), PlatformError> {
        let context = self.store.authenticate(token)?;
        if context.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        tx.execute(
            "UPDATE sessions SET revoked_at=now() WHERE id=$1",
            &[&context.credential_id()],
        )
        .map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(context.actor_id()),
            Some(context.credential_id()),
            "session.revoked",
            Some(context.credential_id()),
            json!({}),
        )?;
        tx.commit().map_err(PlatformStore::database_error)
    }
    pub fn create_actor(
        &mut self,
        ctx: &AuthContext,
        kind: ActorKind,
        name: &str,
        purpose: &str,
    ) -> Result<Actor, PlatformError> {
        if kind == ActorKind::Human {
            return Err(PlatformError::Validation(
                "create human identities through user registration",
            ));
        }
        self.insert(ctx, kind, name, purpose, None)
    }
    pub fn create_user(
        &mut self,
        ctx: &AuthContext,
        username: &str,
        password: &str,
        name: &str,
    ) -> Result<Actor, PlatformError> {
        let current = self.store.fresh(ctx)?;
        current.require(Capability::ActorsManage, None)?;
        let username = DomainText::username(username)?;
        let hash = PasswordCredential::hash(password)?;
        self.insert(
            ctx,
            ActorKind::Human,
            name,
            "Local user",
            Some((username, hash)),
        )
    }
    fn insert(
        &mut self,
        ctx: &AuthContext,
        kind: ActorKind,
        name: &str,
        purpose: &str,
        user: Option<(String, String)>,
    ) -> Result<Actor, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ActorsManage, None)?;
        let actor = Actor::new(SecretGenerator::id("act")?, kind, name, purpose)?;
        let kind = serde_json::to_value(kind).map_err(|_| PlatformError::Storage)?;
        let kind = kind.as_str().ok_or(PlatformError::Storage)?;
        let request = self.store.request_context.clone();
        let operation = if user.is_some() {
            "user.create"
        } else {
            "actor.create"
        };
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

        tx.execute(
            "INSERT INTO actors(id,kind,name,purpose) VALUES($1,$2,$3,$4)",
            &[&actor.id, &kind, &actor.name, &actor.purpose],
        )
        .map_err(PlatformStore::database_error)?;
        if let Some((username, hash)) = user {
            tx.execute(
                "INSERT INTO users(actor_id,username,password_hash) VALUES($1,$2,$3)",
                &[&actor.id, &username, &hash],
            )
            .map_err(PlatformStore::database_error)?;
        }
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "actor.created",
            Some(&actor.id),
            json!({"kind":kind}),
        )?;
        let value: Value = tx
            .query_one("SELECT to_jsonb(a) FROM actors a WHERE id=$1", &[&actor.id])
            .map_err(PlatformStore::database_error)?
            .get(0);
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
    pub fn suspend(&mut self, ctx: &AuthContext, id: &str) -> Result<(), PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ActorsManage, None)?;
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
            "actor.suspend",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            let _: () = PlatformStore::decode(value)?;
            return Ok(());
        }

        let count=tx.execute("UPDATE actors SET active=false,revision=revision+1,updated_at=now() WHERE id=$1 AND active",&[&id]).map_err(PlatformStore::database_error)?;
        if count == 0
            && tx
                .query_opt("SELECT id FROM actors WHERE id=$1", &[&id])
                .map_err(PlatformStore::database_error)?
                .is_none()
        {
            return Err(PlatformError::NotFound);
        }
        if count > 0 {
            AuditWriter::append(
                &mut tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                "actor.suspended",
                Some(id),
                json!({}),
            )?;
        }
        ReceiptWriter::save(&mut tx, request.as_ref(), &serde_json::Value::Null)?;
        tx.commit().map_err(PlatformStore::database_error)
    }
}
