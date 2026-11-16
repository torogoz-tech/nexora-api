use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    git_author_identity::GitAuthorIdentity,
    git_author_observation_writer::GitAuthorObservationWriter, git_author_policy::GitAuthorPolicy,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct GitAuthorAliasStore;
impl GitAuthorAliasStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        actor: &str,
        input: &GitAuthorAliasCreate,
    ) -> Result<Value, PlatformError> {
        if !matches!(input.provider.as_str(), "github" | "gitlab") {
            return Err(PlatformError::Validation("invalid Git author provider"));
        }
        let host = GitAuthorIdentity::host(&input.host)?;
        let email = GitAuthorIdentity::email(&input.email)
            .ok_or(PlatformError::Validation("invalid Git author email"))?;
        let reason = DomainText::optional(&input.reason, 4000)?;
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        GitAuthorPolicy::lock(&mut tx)?;
        GitAuthorPolicy::manager(&mut tx, ctx, None)?;
        GitAuthorPolicy::target(&mut tx, actor, None)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "git.author_alias.create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(v);
        }
        if tx.query_one("SELECT EXISTS(SELECT 1 FROM git_author_aliases WHERE provider=$1 AND host=$2 AND email=$3 AND revoked_at IS NULL)",&[&input.provider,&host,&email]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("Git author alias already exists in the organization"))}
        let id = SecretGenerator::id("gaa")?;
        tx.execute("INSERT INTO git_author_aliases(id,actor_id,provider,host,email,project_id,created_by) VALUES($1,$2,$3,$4,$5,NULL,$6)",&[&id,&actor,&input.provider,&host,&email,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        GitAuthorObservationWriter::attribute(&mut tx, Some(&id), None)?;
        let result = Self::row(&mut tx, &id)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.author_alias.created",
            Some(&id),
            json!({"actor_id":actor,"project_id":null,"provider":input.provider,"host":host,"email":email,"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &result)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn revoke(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        actor: &str,
        id: &str,
        input: &GitAuthorAliasRevoke,
    ) -> Result<Value, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation("invalid Git alias revision"));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        GitAuthorPolicy::lock(&mut tx)?;
        let row=tx.query_opt("SELECT actor_id,project_id,revision,revoked_at IS NOT NULL FROM git_author_aliases WHERE id=$1 FOR UPDATE",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, String>(0) != actor {
            return Err(PlatformError::NotFound);
        }
        let project: Option<String> = row.get(1);
        GitAuthorPolicy::manager(&mut tx, ctx, project.as_deref())?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "git.author_alias.revoke",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(v);
        }
        if row.get::<_, i64>(2) != input.expected_revision || row.get::<_, bool>(3) {
            return Err(PlatformError::Conflict(
                "Git author alias changed or revoked",
            ));
        }
        tx.execute("UPDATE git_author_aliases SET revoked_at=clock_timestamp(),revoked_by=$2,revocation_reason=$3,revision=revision+1 WHERE id=$1",&[&id,&ctx.actor_id(),&reason]).map_err(PlatformStore::database_error)?;
        let result = Self::row(&mut tx, id)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.author_alias.revoked",
            Some(id),
            json!({"actor_id":actor,"project_id":project,"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &result)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    fn row(tx: &mut postgres::Transaction<'_>, id: &str) -> Result<Value, PlatformError> {
        Ok(tx.query_one("SELECT to_jsonb(a)||jsonb_build_object('actor_name',m.name) FROM git_author_aliases a JOIN actors m ON m.id=a.actor_id WHERE a.id=$1",&[&id]).map_err(PlatformStore::database_error)?.get(0))
    }
}
