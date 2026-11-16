use crate::{
    PlatformStore, git_author_policy::GitAuthorPolicy, platform_authority::PlatformAuthority,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct GitAuthorQueryStore;
impl GitAuthorQueryStore {
    fn validate(q: &GitAuthorQuery) -> Result<(), PlatformError> {
        if [&q.project_id, &q.member_id, &q.search, &q.after]
            .iter()
            .filter_map(|v| v.as_deref())
            .any(|s| s.chars().any(char::is_control))
            || !(1..=100).contains(&q.limit)
            || q.search.as_ref().is_some_and(|s| s.len() > 256)
            || q.after
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 128)
        {
            return Err(PlatformError::Validation("invalid Git author query"));
        }
        Ok(())
    }
    fn global(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
    ) -> Result<bool, PlatformError> {
        match PlatformAuthority::require(tx, ctx, Capability::RepositoriesRead, None) {
            Ok(()) => Ok(true),
            Err(PlatformError::Forbidden) => Ok(false),
            Err(e) => Err(e),
        }
    }
    pub fn aliases(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        q: &GitAuthorQuery,
    ) -> Result<Value, PlatformError> {
        Self::validate(q)?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let projects = GitAuthorPolicy::projects(&mut tx, ctx, q.project_id.as_deref())?;
        let manager = match GitAuthorPolicy::manager(&mut tx, ctx, None) {
            Ok(()) => true,
            Err(PlatformError::Forbidden) => false,
            Err(error) => return Err(error),
        };
        let global = Self::global(&mut tx, ctx)? || manager;
        let own =
            ctx.method() == AuthMethod::Session && q.member_id.as_deref() == Some(ctx.actor_id());
        let rows=tx.query("SELECT to_jsonb(a)||jsonb_build_object('actor_name',m.name) FROM git_author_aliases a JOIN actors m ON m.id=a.actor_id WHERE ($1::text IS NULL OR a.project_id=$1 OR a.project_id IS NULL) AND ($2::text IS NULL OR a.actor_id=$2) AND ($3::text IS NULL OR a.id>$3) AND (a.project_id=ANY($4) OR ($5 AND a.project_id IS NULL) OR ($6 AND a.actor_id=$7) OR (a.project_id IS NULL AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=a.actor_id AND g.capability='repositories:read' AND (g.project_id=ANY($4) OR (g.project_id IS NULL AND cardinality($4)>0))))) AND (strpos(lower(a.email),lower($9))>0 OR strpos(lower(m.name),lower($9))>0) ORDER BY a.id LIMIT $8",&[&q.project_id,&q.member_id,&q.after,&projects,&global,&own,&ctx.actor_id(),&(i64::from(q.limit)+1),&q.search.as_deref().unwrap_or("")]).map_err(PlatformStore::database_error)?;
        let result = Self::page(rows, q.limit);
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn authors(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        q: &GitAuthorQuery,
    ) -> Result<Value, PlatformError> {
        Self::validate(q)?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let projects = GitAuthorPolicy::projects(&mut tx, ctx, q.project_id.as_deref())?;
        let pattern = q.search.as_deref().unwrap_or("");
        let rows=tx.query("SELECT jsonb_build_object('id',c.id,'provider',c.provider,'host',c.host,'project_id',c.project_id,'repository_id',c.repository_id,'commit_sha',c.commit_sha,'author_name',c.author_name,'author_email',c.author_email,'actor_id',c.actor_id,'actor_name',a.name,'alias_id',c.alias_id,'occurred_at',c.occurred_at) FROM git_commit_authorship c JOIN project_repositories r ON r.id=c.repository_id JOIN git_connections connection ON connection.id=r.connection_id JOIN actors owner ON owner.id=connection.owner_actor_id AND owner.active LEFT JOIN actors a ON a.id=c.actor_id WHERE c.project_id=ANY($1) AND r.authorization_status='authorized' AND NOT r.archived AND connection.status='active' AND ($2::text IS NULL OR c.id>$2) AND ($3::text IS NULL OR c.actor_id=$3) AND (NOT $6::boolean OR (c.actor_id IS NULL AND c.author_email IS NOT NULL AND NOT EXISTS(SELECT 1 FROM git_author_aliases ga WHERE ga.revoked_at IS NULL AND ga.provider=c.provider AND ga.host=c.host AND ga.email=c.author_email AND (ga.project_id IS NULL OR ga.project_id=c.project_id)))) AND (NOT $6::boolean OR $7::boolean = EXISTS(SELECT 1 FROM git_author_archives ga WHERE ga.project_id=c.project_id AND ga.provider=c.provider AND ga.host=c.host AND ga.email=c.author_email)) AND (strpos(lower(c.author_name),lower($4))>0 OR strpos(lower(COALESCE(c.author_email,'')),lower($4))>0 OR strpos(c.commit_sha,lower($4))>0) ORDER BY c.id LIMIT $5",&[&projects,&q.after,&q.member_id,&pattern,&(i64::from(q.limit)+1),&q.unlinked_only,&q.archived_only]).map_err(PlatformStore::database_error)?;
        let result = Self::page(rows, q.limit);
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn options(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let mut projects = GitAuthorPolicy::projects(&mut tx, ctx, project)?;
        let global = match GitAuthorPolicy::manager(&mut tx, ctx, None) {
            Ok(()) => true,
            Err(PlatformError::Forbidden) => false,
            Err(e) => return Err(e),
        };
        if !global {
            projects=tx.query("SELECT project_id FROM member_role_projects WHERE actor_id=$1 AND project_id=ANY($2) ORDER BY project_id",&[&ctx.actor_id(),&projects]).map_err(PlatformStore::database_error)?.iter().map(|r|r.get::<_,String>(0)).collect();
        }
        let manage = if ctx.method() != AuthMethod::Session {
            false
        } else if let Some(p) = project {
            match GitAuthorPolicy::manager(&mut tx, ctx, Some(p)) {
                Ok(()) => true,
                Err(PlatformError::Forbidden) => false,
                Err(e) => return Err(e),
            }
        } else {
            global||tx.query_one("SELECT EXISTS(SELECT 1 FROM actor_roles r WHERE r.actor_id=$1 AND r.role IN ('pmo','manager','technical_lead','super_admin') AND EXISTS(SELECT 1 FROM member_role_projects p WHERE p.actor_id=r.actor_id AND p.project_id=ANY($2)))",&[&ctx.actor_id(),&projects]).map_err(PlatformStore::database_error)?.get::<_,bool>(0)
        };
        let actors = if manage {
            tx.query("SELECT DISTINCT a.id,a.name,u.username FROM actors a JOIN users u ON u.actor_id=a.id WHERE a.active AND a.kind='human' AND ($2 OR EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=a.id AND g.capability='repositories:read' AND (g.project_id IS NULL OR g.project_id=ANY($1)))) ORDER BY a.name,a.id LIMIT 500",&[&projects,&global]).map_err(PlatformStore::database_error)?.iter().map(|r|json!({"id":r.get::<_,String>(0),"name":r.get::<_,String>(1),"username":r.get::<_,String>(2)})).collect::<Vec<_>>()
        } else {
            vec![]
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(json!({"actors":actors,"can_manage":manage,"can_manage_global":global}))
    }
    fn page(rows: Vec<postgres::Row>, limit: u16) -> Value {
        let more = rows.len() > usize::from(limit);
        let items = rows
            .into_iter()
            .take(usize::from(limit))
            .map(|r| r.get::<_, Value>(0))
            .collect::<Vec<_>>();
        let after = if more {
            items
                .last()
                .and_then(|v| v["id"].as_str())
                .map(str::to_owned)
        } else {
            None
        };
        json!({"items":items,"has_more":more,"next_after":after})
    }
}
