use crate::receipt_writer::ReceiptWriter;
use crate::{PlatformStore, SecretGenerator, audit_writer::AuditWriter};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct CredentialStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> CredentialStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    fn manage(&mut self, ctx: &AuthContext, actor: &str) -> Result<AuthContext, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::KeysManage, None)?;
        if ctx.actor_id() != actor {
            ctx.require(Capability::ActorsManage, None)?;
        }
        Ok(ctx)
    }
    fn valid_lifetime(lifetime: i64) -> Result<(), PlatformError> {
        if !(60..=31_536_000).contains(&lifetime) {
            return Err(PlatformError::Validation(
                "key lifetime must be 60..31536000 seconds",
            ));
        }
        Ok(())
    }
    fn validate_scope(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        scope: &KeyScope,
    ) -> Result<(), PlatformError> {
        scope.validate()?;
        if self
            .store
            .client
            .query_opt("SELECT id FROM actors WHERE id=$1 AND active", &[&actor])
            .map_err(PlatformStore::database_error)?
            .is_none()
        {
            return Err(PlatformError::NotFound);
        }
        for project in &scope.project_ids {
            if self
                .store
                .client
                .query_opt(
                    "SELECT id FROM projects WHERE id=$1 AND NOT archived",
                    &[&project],
                )
                .map_err(PlatformStore::database_error)?
                .is_none()
            {
                return Err(PlatformError::NotFound);
            }
        }
        for cap in &scope.capabilities {
            let mut targets: Vec<Option<&str>> =
                scope.project_ids.iter().map(|s| Some(s.as_str())).collect();
            if scope.installation || scope.all_projects {
                targets.push(None);
            }
            for target in targets {
                ctx.require(*cap, target)?;
                let exists=self.store.client.query_one("SELECT EXISTS(SELECT 1 FROM actor_grants WHERE actor_id=$1 AND capability=$2 AND (project_id IS NULL OR project_id=$3))",&[&actor,&cap.as_str(),&target]).map_err(PlatformStore::database_error)?.get::<_,bool>(0);
                if !exists {
                    return Err(PlatformError::Forbidden);
                }
            }
        }
        Ok(())
    }
    pub fn issue(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        name: &str,
        scope: KeyScope,
        lifetime: i64,
    ) -> Result<IssuedApiKey, PlatformError> {
        let ctx = self.manage(ctx, actor)?;
        self.validate_scope(&ctx, actor, &scope)?;
        Self::valid_lifetime(lifetime)?;
        let name = DomainText::name(name)?;
        let id = SecretGenerator::id("key")?;
        let key = format!("nxk_{id}.{}", SecretGenerator::token("secret")?);
        let digest = SecretGenerator::digest(&key);
        let scope = serde_json::to_value(scope).map_err(|_| PlatformError::Storage)?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(_value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "key.issue",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Err(PlatformError::Conflict(
                "credential already issued; list its metadata or rotate instead of recovering its secret",
            ));
        }

        tx.execute("INSERT INTO api_keys(id,actor_id,name,digest,scope,expires_at) VALUES($1,$2,$3,$4,$5,now()+$6::bigint*interval '1 second')",&[&id,&actor,&name,&digest,&scope,&lifetime]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "key.issued",
            Some(&id),
            json!({"owner_actor_id":actor,"scope":scope}),
        )?;
        let metadata: ApiKeyMetadata = PlatformStore::decode(
            tx.query_one(
                "SELECT to_jsonb(k)-'digest' FROM api_keys k WHERE id=$1",
                &[&id],
            )
            .map_err(PlatformStore::database_error)?
            .get(0),
        )?;
        ReceiptWriter::save(
            &mut tx,
            request.as_ref(),
            &json!({"metadata":metadata,"secret_retrievable":false}),
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(IssuedApiKey { key, metadata })
    }
    pub fn list(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
    ) -> Result<Vec<ApiKeyMetadata>, PlatformError> {
        self.manage(ctx, actor)?;
        self.store.client.query("SELECT to_jsonb(k)-'digest' FROM api_keys k WHERE actor_id=$1 ORDER BY created_at,id LIMIT 1000",&[&actor]).map_err(PlatformStore::database_error)?.iter().map(|r|PlatformStore::decode(r.get(0))).collect()
    }
    fn metadata(&mut self, id: &str) -> Result<ApiKeyMetadata, PlatformError> {
        let row = self
            .store
            .client
            .query_opt(
                "SELECT to_jsonb(k)-'digest' FROM api_keys k WHERE id=$1",
                &[&id],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        PlatformStore::decode(row.get(0))
    }
    pub fn revoke(&mut self, ctx: &AuthContext, id: &str) -> Result<(), PlatformError> {
        let metadata = self.metadata(id)?;
        let ctx = self.manage(ctx, &metadata.actor_id)?;
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
            "key.revoke",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            let _: () = PlatformStore::decode(value)?;
            return Ok(());
        }

        let n = tx
            .execute(
                "UPDATE api_keys SET revoked_at=now() WHERE id=$1 AND revoked_at IS NULL",
                &[&id],
            )
            .map_err(PlatformStore::database_error)?;
        if n > 0 {
            AuditWriter::append(
                &mut tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                "key.revoked",
                Some(id),
                json!({}),
            )?;
        }
        ReceiptWriter::save(&mut tx, request.as_ref(), &serde_json::Value::Null)?;
        tx.commit().map_err(PlatformStore::database_error)
    }
    pub fn rotate(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        lifetime: i64,
    ) -> Result<IssuedApiKey, PlatformError> {
        let old = self.metadata(id)?;
        let ctx = self.manage(ctx, &old.actor_id)?;
        Self::valid_lifetime(lifetime)?;
        if old.revoked_at.is_some() {
            return Err(PlatformError::Conflict("key already revoked or rotated"));
        }
        self.validate_scope(&ctx, &old.actor_id, &old.scope)?;
        let next = SecretGenerator::id("key")?;
        let key = format!("nxk_{next}.{}", SecretGenerator::token("secret")?);
        let digest = SecretGenerator::digest(&key);
        let scope = serde_json::to_value(&old.scope).map_err(|_| PlatformError::Storage)?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(_value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "key.rotate",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Err(PlatformError::Conflict(
                "credential already issued; list its metadata or rotate instead of recovering its secret",
            ));
        }

        let n = tx
            .execute(
                "UPDATE api_keys SET revoked_at=now() WHERE id=$1 AND revoked_at IS NULL",
                &[&id],
            )
            .map_err(PlatformStore::database_error)?;
        if n != 1 {
            return Err(PlatformError::Conflict("key already revoked or rotated"));
        }
        tx.execute("INSERT INTO api_keys(id,actor_id,name,digest,scope,expires_at) VALUES($1,$2,$3,$4,$5,now()+$6::bigint*interval '1 second')",&[&next,&old.actor_id,&old.name,&digest,&scope,&lifetime]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "key.rotated",
            Some(id),
            json!({"replacement_key_id":next}),
        )?;
        let value: Value = tx
            .query_one(
                "SELECT to_jsonb(k)-'digest' FROM api_keys k WHERE id=$1",
                &[&next],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        ReceiptWriter::save(
            &mut tx,
            request.as_ref(),
            &json!({"metadata":value,"secret_retrievable":false}),
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(IssuedApiKey {
            key,
            metadata: PlatformStore::decode(value)?,
        })
    }
}
