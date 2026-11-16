use crate::{
    PlatformStore, platform_authority::PlatformAuthority, repository_access::RepositoryAccess,
    repository_job_source::RepositoryJobSource, task_read_authority::TaskReadAuthority,
    task_read_predicate::task_read_predicate,
};
use nexora_platform::{AuthContext, Capability, PlatformError, RepositorySourceKind};
use postgres::Transaction;
use serde_json::{Value, json};

pub(crate) struct ProjectProfileStore;
impl ProjectProfileStore {
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        year: Option<i32>,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::ProjectsRead, Some(project))?;
        // Archived project identity remains readable; active work sections follow existing policy.
        let identity: Value = tx.query_opt("SELECT jsonb_build_object('id',id,'name',name,'description',description,'archived',archived,'revision',revision,'created_at',created_at) FROM projects WHERE id=$1 FOR SHARE", &[&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        let archived = identity["archived"].as_bool().unwrap_or(true);
        let current_year: i32 = tx
            .query_one(
                "SELECT extract(year FROM clock_timestamp() AT TIME ZONE 'UTC')::integer",
                &[],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        let year = year.unwrap_or(current_year);
        if !(2000..=current_year).contains(&year) {
            return Err(PlatformError::Validation("invalid profile year"));
        }
        let tasks_read = !archived && Self::allows(&mut tx, ctx, Capability::TasksRead, project)?;
        let documents_read =
            !archived && Self::allows(&mut tx, ctx, Capability::DocumentsRead, project)?;
        let repositories_read = !archived
            && Self::allows(&mut tx, ctx, Capability::RepositoriesRead, project)?
            && Self::allows(&mut tx, ctx, Capability::KnowledgeRead, project)?;
        let unrestricted = tasks_read && TaskReadAuthority::unrestricted(&mut tx, ctx, project)?;
        let mut repositories = Vec::new();
        let mut repository_ids: Vec<String> = Vec::new();
        if repositories_read {
            let rows = tx.query("SELECT id,COALESCE(remote_full_name,local_name,requested_full_name),provider,authorization_status FROM project_repositories WHERE project_id=$1 AND NOT archived AND authorization_status='authorized' ORDER BY updated_at DESC,id", &[&project]).map_err(PlatformStore::database_error)?;
            for row in rows {
                let id: String = row.get(0);
                let access = match RepositoryAccess::lock(&mut tx, ctx, project, &id) {
                    Ok(access) => access,
                    Err(PlatformError::NotFound | PlatformError::Forbidden) => continue,
                    Err(error) => return Err(error),
                };
                if access.source_kind == RepositorySourceKind::Git {
                    match RepositoryJobSource::lock(&mut tx, ctx, project, &id, access.revision) {
                        Ok(_) => {},
                        Err(PlatformError::NotFound | PlatformError::Forbidden | PlatformError::Conflict(_)) => continue,
                        Err(error) => return Err(error),
                    }
                } else if tx.query_opt("SELECT id FROM project_repositories WHERE id=$1 AND project_id=$2 AND authorization_status='authorized' AND NOT archived FOR SHARE", &[&id,&project]).map_err(PlatformStore::database_error)?.is_none() { continue; }
                repository_ids.push(id.clone());
                if repositories.len() < 6 {
                    repositories.push(json!({"id":id,"project_id":project,"name":row.get::<_,String>(1),"provider":row.get::<_,Option<String>>(2),"status":row.get::<_,String>(3)}));
                }
            }
        }
        // Project directory exposes only basic identity of active humans with current project access.
        tx.query("SELECT actor_id FROM actor_grants WHERE capability='projects:read' AND (project_id IS NULL OR project_id=$1) FOR SHARE", &[&project]).map_err(PlatformStore::database_error)?;
        let members: Vec<Value> = tx.query("SELECT jsonb_build_object('id',a.id,'name',a.name,'username',u.username,'position',COALESCE((SELECT position FROM member_positions WHERE actor_id=a.id),''),'role',(SELECT role FROM actor_roles WHERE actor_id=a.id),'avatar_revision',COALESCE((SELECT revision FROM user_avatars WHERE actor_id=a.id),0)) FROM actors a JOIN users u ON u.actor_id=a.id WHERE a.active AND a.kind='human' AND NOT EXISTS(SELECT 1 FROM actor_roles ar WHERE ar.actor_id=a.id AND ar.role='super_admin') AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=a.id AND g.capability='projects:read' AND (g.project_id IS NULL OR g.project_id=$1)) ORDER BY a.name,a.id LIMIT 24 FOR SHARE OF a,u", &[&project]).map_err(PlatformStore::database_error)?.iter().map(|row|row.get(0)).collect();
        let predicate = task_read_predicate("$2", "$3");
        let task_count: i64 = if tasks_read {
            tx.query_one(&format!("SELECT count(*) FROM tasks t WHERE t.project_id=$1 AND NOT t.archived AND {predicate}"), &[&project,&ctx.actor_id(),&unrestricted]).map_err(PlatformStore::database_error)?.get(0)
        } else {
            0
        };
        let priority_tasks: Vec<Value> = if tasks_read {
            tx.query(&format!("SELECT jsonb_build_object('id',t.id,'project_id',t.project_id,'title',t.title,'priority',t.priority) FROM tasks t WHERE t.project_id=$1 AND NOT t.archived AND t.priority IN ('high','critical') AND {predicate} AND NOT EXISTS(SELECT 1 FROM workflow_versions w CROSS JOIN LATERAL jsonb_array_elements(w.graph->'states') s WHERE w.id=t.workflow_version_id AND s->>'id'=t.state_id AND s->>'terminal'='true') ORDER BY CASE t.priority WHEN 'critical' THEN 0 ELSE 1 END,t.updated_at DESC,t.id LIMIT 8"), &[&project,&ctx.actor_id(),&unrestricted]).map_err(PlatformStore::database_error)?.iter().map(|row|row.get(0)).collect()
        } else {
            Vec::new()
        };
        let (documents, requirements): (i64, i64) = if documents_read {
            let row = tx.query_one("SELECT count(*) FILTER(WHERE kind<>'requirement'),count(*) FILTER(WHERE kind='requirement') FROM project_documents WHERE project_id=$1 AND NOT archived", &[&project]).map_err(PlatformStore::database_error)?;
            (row.get(0), row.get(1))
        } else {
            (0, 0)
        };
        // Feature catalog reads require both documents and tasks capabilities.
        let features: i64 = if documents_read && tasks_read {
            tx.query_one(
                "SELECT count(*) FROM project_features WHERE project_id=$1 AND NOT archived",
                &[&project],
            )
            .map_err(PlatformStore::database_error)?
            .get(0)
        } else {
            0
        };
        let events = format!(
            "SELECT h.id,h.project_id,'task'::text AS kind,t.title,h.created_at FROM task_history h JOIN tasks t ON t.id=h.task_id AND t.project_id=h.project_id WHERE h.project_id=$1 AND $4 AND NOT t.archived AND {predicate} UNION ALL SELECT v.id,v.project_id,'document'::text,v.title,v.created_at FROM project_document_versions v JOIN project_documents d ON d.id=v.document_id AND d.project_id=v.project_id WHERE v.project_id=$1 AND $5 AND NOT d.archived UNION ALL SELECT * FROM (SELECT DISTINCT ON (g.provider,g.host,g.remote_repository_id,g.commit_sha) g.id,g.project_id,'commit'::text AS kind,COALESCE(NULLIF(g.commit_message,''),g.commit_sha) AS title,g.occurred_at AS created_at FROM git_commit_authorship g JOIN project_repositories r ON r.id=g.repository_id AND r.project_id=g.project_id JOIN git_connections c ON c.id=r.connection_id JOIN actors owner ON owner.id=c.owner_actor_id AND owner.active WHERE g.project_id=$1 AND g.repository_id=ANY($7) AND NOT r.archived AND r.authorization_status='authorized' AND c.status='active' ORDER BY g.provider,g.host,g.remote_repository_id,g.commit_sha,g.occurred_at,g.id) git_events"
        );
        let parameters: &[&(dyn postgres::types::ToSql + Sync)] = &[
            &project,
            &ctx.actor_id(),
            &unrestricted,
            &tasks_read,
            &documents_read,
            &year,
            &repository_ids,
        ];
        let contributions: Vec<Value> = tx.query(&format!("WITH events AS ({events}) SELECT jsonb_build_object('date',to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD'),'count',count(*)) FROM events WHERE created_at>=make_timestamptz($6,1,1,0,0,0,'UTC') AND created_at<make_timestamptz($6+1,1,1,0,0,0,'UTC') GROUP BY to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD') ORDER BY to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD')"), parameters).map_err(PlatformStore::database_error)?.iter().map(|row|row.get(0)).collect();
        let activity: Vec<Value> = tx.query(&format!("WITH events AS ({events}) SELECT jsonb_build_object('id',id,'project_id',project_id,'kind',kind,'title',title,'created_at',created_at,'repository_id',(SELECT g.repository_id FROM git_commit_authorship g WHERE g.id=events.id),'repository_name',(SELECT COALESCE(r.remote_full_name,r.local_name,r.requested_full_name) FROM git_commit_authorship g JOIN project_repositories r ON r.id=g.repository_id WHERE g.id=events.id),'commit_sha',(SELECT g.commit_sha FROM git_commit_authorship g WHERE g.id=events.id),'author',(SELECT jsonb_build_object('id',a.id,'name',COALESCE(a.name,NULLIF(g.author_name,''),'Autor desconocido'),'position',COALESCE((SELECT position FROM member_positions WHERE actor_id=a.id),''),'avatar_revision',COALESCE((SELECT revision FROM user_avatars WHERE actor_id=a.id),0)) FROM git_commit_authorship g LEFT JOIN actors a ON a.id=g.actor_id AND a.kind='human' AND EXISTS(SELECT 1 FROM users u WHERE u.actor_id=a.id) AND NOT EXISTS(SELECT 1 FROM actor_roles ar WHERE ar.actor_id=a.id AND ar.role='super_admin') WHERE events.kind='commit' AND g.id=events.id),'branches',COALESCE((SELECT jsonb_agg(b.branch_name ORDER BY b.branch_name) FROM git_commit_branches b JOIN git_commit_authorship g ON g.repository_id=b.repository_id AND g.commit_sha=b.commit_sha WHERE g.id=events.id),'[]'::jsonb)) FROM events WHERE created_at>=make_timestamptz($6,1,1,0,0,0,'UTC') AND created_at<make_timestamptz($6+1,1,1,0,0,0,'UTC') ORDER BY created_at DESC,id DESC LIMIT 100"), parameters).map_err(PlatformStore::database_error)?.iter().map(|row|row.get(0)).collect();
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(
            json!({"project":identity,"repositories":repositories,"members":members,"priority_tasks":priority_tasks,"counts":{"tasks":task_count,"documents":documents,"requirements":requirements,"features":features},"contributions":contributions,"activity":activity,"year":year}),
        )
    }
    fn allows(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        capability: Capability,
        project: &str,
    ) -> Result<bool, PlatformError> {
        match PlatformAuthority::require(tx, ctx, capability, Some(project)) {
            Ok(()) => Ok(true),
            Err(PlatformError::Forbidden) => Ok(false),
            Err(error) => Err(error),
        }
    }
}
