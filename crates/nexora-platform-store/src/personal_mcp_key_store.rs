use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    personal_mcp_key_policy::PersonalMcpKeyPolicy, receipt_writer::ReceiptWriter,
};
use nexora_platform::{
    AuthContext, AuthMethod, DomainText, IssuedApiKey, PersonalMcpKeyCreate,
    PersonalMcpKeyListQuery, PlatformError,
};
use serde_json::{Value, json};

pub(crate) struct PersonalMcpKeyStore;
impl PersonalMcpKeyStore {
    pub fn list(store: &mut PlatformStore, ctx: &AuthContext) -> Result<Value, PlatformError> {
        Self::list_page(store, ctx, None)
    }
    pub fn list_page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        query: Option<&PersonalMcpKeyListQuery>,
    ) -> Result<Value, PlatformError> {
        let limit = query.map(|q| q.limit.unwrap_or(20)).unwrap_or(1000);
        let status = query.and_then(|q| q.status.as_deref()).unwrap_or("all");
        let after = query.and_then(|q| q.after.as_deref());
        if limit == 0
            || (query.is_some() && limit > 20)
            || !["all", "active", "inactive"].contains(&status)
            || after.is_some_and(|id| id.is_empty() || id.len() > 160)
        {
            return Err(PlatformError::Validation("invalid personal MCP key page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PersonalMcpKeyPolicy::session(&mut tx, ctx)?;
        if let Some(after) = after
            && tx
                .query_opt(
                    "SELECT id FROM api_keys WHERE id=$1 AND actor_id=$2",
                    &[&after, &ctx.actor_id()],
                )
                .map_err(PlatformStore::database_error)?
                .is_none()
        {
            return Err(PlatformError::Validation("invalid personal MCP key cursor"));
        }
        let rows=tx.query("SELECT to_jsonb(k)-'digest' FROM api_keys k WHERE actor_id=$1 AND ($2='all' OR ($2='active' AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>now())) OR ($2='inactive' AND (revoked_at IS NOT NULL OR expires_at IS NOT NULL AND expires_at<=now()))) AND ($3::text IS NULL OR (created_at,id)<(SELECT created_at,id FROM api_keys WHERE actor_id=$1 AND id=$3)) ORDER BY created_at DESC,id DESC LIMIT $4",&[&ctx.actor_id(),&status,&after,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let has_more = rows.len() > limit as usize;
        let items = rows
            .iter()
            .take(limit as usize)
            .map(|r| r.get::<_, Value>(0))
            .collect::<Vec<_>>();
        let next_after = if has_more {
            items.last().and_then(|v| v["id"].as_str())
        } else {
            None
        };
        let projects=tx.query("SELECT jsonb_build_object('id',p.id,'name',p.name) FROM projects p WHERE NOT p.archived AND NOT EXISTS(SELECT 1 FROM actor_roles ar WHERE ar.actor_id=$1 AND ar.role IN ('user','supervisor')) AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability IN ('projects:read','repositories:read','tasks:read','documents:read','knowledge:read') AND (g.project_id IS NULL OR g.project_id=p.id)) ORDER BY p.name,p.id LIMIT 256",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?.iter().map(|r|r.get::<_,Value>(0)).collect::<Vec<_>>();
        let global = tx
            .query(
                "SELECT capability FROM actor_grants WHERE actor_id=$1 AND project_id IS NULL",
                &[&ctx.actor_id()],
            )
            .map_err(PlatformStore::database_error)?
            .iter()
            .any(|r| {
                nexora_platform::Capability::all().into_iter().any(|c| {
                    c.as_str() == r.get::<_, String>(0) && PersonalMcpKeyPolicy::safe(c, false)
                })
            });
        let now: String = tx
            .query_one("SELECT clock_timestamp()::text", &[])
            .map_err(PlatformStore::database_error)?
            .get(0);
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(
            json!({"items":items,"has_more":has_more,"next_after":next_after,"projects":projects,"mcp_url":"/mcp","server_time":now,"can_create":!projects.is_empty()||global,"can_include_global":global}),
        )
    }
    pub fn issue(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        input: &PersonalMcpKeyCreate,
    ) -> Result<IssuedApiKey, PlatformError> {
        if input.expires_in_seconds != 0 && !(3600..=7_776_000).contains(&input.expires_in_seconds)
        {
            return Err(PlatformError::Validation(
                "personal MCP key lifetime must be 1 hour to 90 days",
            ));
        }
        let name = DomainText::name(&input.name)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        tx.query_one("SELECT pg_advisory_xact_lock(hashtextextended(current_schema()||':personal-mcp-key:'||$1,0))",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        PersonalMcpKeyPolicy::session(&mut tx, ctx)?;
        let scope = PersonalMcpKeyPolicy::scope(&mut tx, ctx, input)?;
        if ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "mcp.key.issue",
        )?
        .is_some()
        {
            return Err(PlatformError::Conflict(
                "credential already issued; its secret cannot be recovered",
            ));
        }
        let count:i64=tx.query_one("SELECT count(*) FROM api_keys WHERE actor_id=$1 AND personal_mcp AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>clock_timestamp())",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?.get(0);
        if count >= 20 {
            return Err(PlatformError::Validation(
                "maximum 20 active personal MCP keys",
            ));
        }
        let id = SecretGenerator::id("key")?;
        let key = format!("nxk_{id}.{}", SecretGenerator::token("secret")?);
        let digest = SecretGenerator::digest(&key);
        let scope = serde_json::to_value(scope).map_err(|_| PlatformError::Storage)?;
        tx.execute("INSERT INTO api_keys(id,actor_id,name,digest,scope,expires_at,personal_mcp) VALUES($1,$2,$3,$4,$5,CASE WHEN $6::bigint=0 THEN NULL ELSE clock_timestamp()+$6::bigint*interval '1 second' END,true)",&[&id,&ctx.actor_id(),&name,&digest,&scope,&input.expires_in_seconds]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "mcp.key.issued",
            Some(&id),
            json!({"scope":scope}),
        )?;
        let metadata = PlatformStore::decode(
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
            &json!({"key_id":id,"secret_retrievable":false}),
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(IssuedApiKey { key, metadata })
    }
    pub fn revoke(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<(), PlatformError> {
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PersonalMcpKeyPolicy::session(&mut tx, ctx)?;
        tx.query_opt(
            "SELECT id FROM api_keys WHERE id=$1 AND actor_id=$2 FOR UPDATE",
            &[&id, &ctx.actor_id()],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        if ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "mcp.key.revoke",
        )?
        .is_some()
        {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(());
        }
        tx.execute("UPDATE api_keys SET revoked_at=COALESCE(revoked_at,clock_timestamp()) WHERE id=$1 AND actor_id=$2",&[&id,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "mcp.key.revoked",
            Some(id),
            json!({}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &Value::Null)?;
        tx.commit().map_err(PlatformStore::database_error)
    }
    pub fn activity(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        client: Option<(String, String)>,
    ) -> Result<(), PlatformError> {
        if ctx.method() != AuthMethod::ApiKey {
            return Err(PlatformError::Forbidden);
        }
        let client = client.filter(|(name, version)| {
            !name.is_empty()
                && name.len() <= 128
                && version.len() <= 64
                && !name.chars().chain(version.chars()).any(char::is_control)
        });
        let (name, version) = client.map_or((None, None), |(n, v)| (Some(n), Some(v)));
        let updated=store.client.execute("UPDATE api_keys k SET last_mcp_at=clock_timestamp(),mcp_client_name=COALESCE($3,mcp_client_name),mcp_client_version=COALESCE($4,mcp_client_version) WHERE k.id=$1 AND k.actor_id=$2 AND k.revoked_at IS NULL AND (k.expires_at IS NULL OR k.expires_at>clock_timestamp()) AND EXISTS(SELECT 1 FROM actors a WHERE a.id=k.actor_id AND a.active) AND ($3::text IS NOT NULL OR last_mcp_at IS NULL OR last_mcp_at<clock_timestamp()-interval '60 seconds')",&[&ctx.credential_id(),&ctx.actor_id(),&name,&version]).map_err(PlatformStore::database_error)?;
        let _ = updated;
        Ok(())
    }
}
