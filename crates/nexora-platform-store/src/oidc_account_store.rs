use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct OidcAccountStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> OidcAccountStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    fn subject(value: &str) -> Result<(), PlatformError> {
        if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
            return Err(PlatformError::Validation("invalid corporate subject"));
        }
        Ok(())
    }
    pub fn create_user(
        &mut self,
        ctx: &AuthContext,
        input: &OidcUserRegistration,
    ) -> Result<Actor, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ActorsManage, None)?;
        Self::subject(&input.subject)?;
        let username = DomainText::username(&input.username)?;
        let name = DomainText::name(&input.name)?;
        let id = SecretGenerator::id("act")?;
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
            "user.oidc.create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let issuer: String = tx
            .query_opt(
                "SELECT issuer FROM oidc_providers WHERE id=$1",
                &[&input.provider_id],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        tx.execute("INSERT INTO actors(id,kind,name,purpose) VALUES($1,'human',$2,'Corporate OIDC account')",&[&id,&name]).map_err(PlatformStore::database_error)?;
        tx.execute(
            "INSERT INTO users(actor_id,username) VALUES($1,$2)",
            &[&id, &username],
        )
        .map_err(PlatformStore::database_error)?;
        tx.execute(
            "INSERT INTO oidc_identities(issuer,subject,provider_id,actor_id) VALUES($1,$2,$3,$4)",
            &[&issuer, &input.subject, &input.provider_id, &id],
        )
        .map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "identity.corporate_user_created",
            Some(&id),
            json!({"provider_id":input.provider_id}),
        )?;
        let value: Value = tx
            .query_one("SELECT to_jsonb(a) FROM actors a WHERE id=$1", &[&id])
            .map_err(PlatformStore::database_error)?
            .get(0);
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
    pub fn assign(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        input: &OidcIdentityAssignment,
    ) -> Result<(), PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ActorsManage, None)?;
        Self::subject(&input.subject)?;
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
            "identity.oidc.assign",
        )? {
            let _: () = PlatformStore::decode(value)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(());
        }
        tx.query_opt("SELECT a.id FROM actors a JOIN users u ON u.actor_id=a.id WHERE a.id=$1 AND a.kind='human' AND a.active",&[&actor]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        for row in tx
            .query(
                "SELECT capability,project_id FROM actor_grants WHERE actor_id=$1",
                &[&actor],
            )
            .map_err(PlatformStore::database_error)?
        {
            let capability: Capability = PlatformStore::decode(Value::String(row.get(0)))?;
            let project: Option<String> = row.get(1);
            ctx.require(capability, project.as_deref())?;
        }
        let issuer: String = tx
            .query_opt(
                "SELECT issuer FROM oidc_providers WHERE id=$1",
                &[&input.provider_id],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        tx.execute(
            "INSERT INTO oidc_identities(issuer,subject,provider_id,actor_id) VALUES($1,$2,$3,$4)",
            &[&issuer, &input.subject, &input.provider_id, &actor],
        )
        .map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "identity.corporate_mapping_created",
            Some(actor),
            json!({"provider_id":input.provider_id}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &Value::Null)?;
        tx.commit().map_err(PlatformStore::database_error)
    }
}
