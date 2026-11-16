use crate::{
    PlatformStore, platform_authority::PlatformAuthority, repository_access::RepositoryAccess,
    repository_job_source::RepositoryJobSource, task_read_authority::TaskReadAuthority,
    task_read_predicate::task_read_predicate,
};
use nexora_platform::{AuthContext, Capability, PlatformError, RepositorySourceKind};
use postgres::Transaction;
use serde_json::{Value, json};

pub(crate) struct RepositoryProfileStore;
impl RepositoryProfileStore {
    pub fn registered(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        year: Option<i32>,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::require(&mut tx, ctx, project, repository)?;
        let metadata: Value = tx.query_one("SELECT jsonb_build_object('id',id,'remote_id',remote_id,'name',COALESCE(remote_full_name,local_name,requested_full_name),'description',NULL,'provider',provider,'status',authorization_status,'default_branch',NULL,'web_url',NULL) FROM project_repositories WHERE id=$1 AND project_id=$2", &[&repository,&project]).map_err(PlatformStore::database_error)?.get(0);
        let mut result =
            Self::assemble(&mut tx, ctx, &[(project.into(), repository.into())], year)?;
        result["repository"] = metadata;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn remote(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        connection: &str,
        remote: &str,
        metadata: Value,
        year: Option<i32>,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let rows = tx.query("SELECT r.project_id,r.id FROM project_repositories r JOIN projects p ON p.id=r.project_id JOIN git_connections c ON c.id=$1 AND r.provider=c.provider AND r.host=c.host WHERE r.remote_id=$2 AND NOT r.archived AND NOT p.archived ORDER BY p.name,r.id", &[&connection,&remote]).map_err(PlatformStore::database_error)?;
        let mut repositories = Vec::new();
        for row in rows {
            let project: String = row.get(0);
            let repository: String = row.get(1);
            match Self::require(&mut tx, ctx, &project, &repository) {
                Ok(()) => repositories.push((project, repository)),
                Err(
                    PlatformError::NotFound | PlatformError::Forbidden | PlatformError::Conflict(_),
                ) => {}
                Err(error) => return Err(error),
            }
        }
        let mut result = Self::assemble(&mut tx, ctx, &repositories, year)?;
        result["repository"] = metadata["repository"].clone();
        result["pull_requests"] = metadata["pull_requests"].clone();
        result["pull_requests_available"] = metadata["pull_requests_available"].clone();
        result["counts"]["pull_requests"] =
            json!(result["pull_requests"].as_array().map_or(0, Vec::len));
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    fn require(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<(), PlatformError> {
        let access = RepositoryAccess::lock(tx, ctx, project, repository)?;
        if access.source_kind == RepositorySourceKind::Git {
            RepositoryJobSource::lock(tx, ctx, project, repository, access.revision)
                .map_err(RepositoryAccess::conceal)?;
        } else {
            tx.query_opt("SELECT id FROM project_repositories WHERE id=$1 AND project_id=$2 AND authorization_status='authorized' AND NOT archived FOR SHARE", &[&repository,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        }
        Ok(())
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
    fn assemble(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        repositories: &[(String, String)],
        year: Option<i32>,
    ) -> Result<Value, PlatformError> {
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
        let mut projects = Vec::new();
        let mut repository_ids: Vec<String> = Vec::new();
        let mut task_projects: Vec<String> = Vec::new();
        let mut unrestricted: Vec<String> = Vec::new();
        let mut document_projects: Vec<String> = Vec::new();
        for (project, repository) in repositories {
            let name: String = tx
                .query_one(
                    "SELECT name FROM projects WHERE id=$1 AND NOT archived",
                    &[&project],
                )
                .map_err(PlatformStore::database_error)?
                .get(0);
            projects.push(json!({"id":project,"name":name,"repository_id":repository}));
            repository_ids.push(repository.clone());
            if Self::allows(tx, ctx, Capability::TasksRead, project)? {
                task_projects.push(project.clone());
                if TaskReadAuthority::unrestricted(tx, ctx, project)? {
                    unrestricted.push(project.clone());
                }
            }
            if Self::allows(tx, ctx, Capability::DocumentsRead, project)? {
                document_projects.push(project.clone());
            }
        }
        let predicate = task_read_predicate("$3", "t.project_id=ANY($4)");
        // Only explicit repository relations attribute a task to this profile.
        let tasks = format!(
            "SELECT t.* FROM tasks t WHERE NOT t.archived AND t.project_id=ANY($2) AND {predicate} AND (EXISTS(SELECT 1 FROM pull_request_links l WHERE l.task_id=t.id AND l.project_id=t.project_id AND l.repository_id=ANY($1) AND l.removed_at IS NULL) OR EXISTS(SELECT 1 FROM task_work_sessions s CROSS JOIN LATERAL jsonb_array_elements(s.record->'branches') b WHERE s.task_id=t.id AND s.project_id=t.project_id AND b->>'repository_id'=ANY($1)))"
        );
        let params: &[&(dyn postgres::types::ToSql + Sync)] = &[
            &repository_ids,
            &task_projects,
            &ctx.actor_id(),
            &unrestricted,
        ];
        let task_count: i64 = tx
            .query_one(
                &format!("WITH visible_tasks AS ({tasks}) SELECT count(*) FROM visible_tasks"),
                params,
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        let priority_tasks: Vec<Value> = tx.query(&format!("WITH visible_tasks AS ({tasks}) SELECT jsonb_build_object('id',t.id,'project_id',t.project_id,'title',t.title,'priority',t.priority) FROM visible_tasks t WHERE t.priority IN ('high','critical') AND NOT EXISTS(SELECT 1 FROM workflow_versions w CROSS JOIN LATERAL jsonb_array_elements(w.graph->'states') s WHERE w.id=t.workflow_version_id AND s->>'id'=t.state_id AND s->>'terminal'='true') ORDER BY CASE t.priority WHEN 'critical' THEN 0 ELSE 1 END,t.updated_at DESC,t.id LIMIT 8"),params).map_err(PlatformStore::database_error)?.iter().map(|row| row.get(0)).collect();
        let events = "SELECT h.id,h.project_id,'task'::text AS kind,t.title,h.created_at FROM task_history h JOIN visible_tasks t ON t.id=h.task_id AND t.project_id=h.project_id UNION ALL SELECT v.id,v.project_id,'document'::text,v.title,v.created_at FROM project_document_versions v JOIN project_documents d ON d.id=v.document_id AND d.project_id=v.project_id AND NOT d.archived WHERE v.project_id=ANY($5) AND EXISTS(SELECT 1 FROM pull_request_links l WHERE l.repository_id=ANY($1) AND l.project_id=v.project_id AND l.document_id=v.document_id AND l.version_id=v.id AND l.removed_at IS NULL) UNION ALL SELECT * FROM (SELECT DISTINCT ON (g.provider,g.host,g.remote_repository_id,g.commit_sha) g.id,g.project_id,'commit'::text AS kind,COALESCE(NULLIF(g.commit_message,''),g.commit_sha) AS title,g.occurred_at AS created_at FROM git_commit_authorship g JOIN project_repositories repository ON repository.id=g.repository_id JOIN git_connections connection ON connection.id=repository.connection_id JOIN actors owner ON owner.id=connection.owner_actor_id AND owner.active WHERE g.repository_id=ANY($1) AND NOT repository.archived AND repository.authorization_status='authorized' AND connection.status='active' ORDER BY g.provider,g.host,g.remote_repository_id,g.commit_sha,g.occurred_at,g.id) git_events";
        let params: &[&(dyn postgres::types::ToSql + Sync)] = &[
            &repository_ids,
            &task_projects,
            &ctx.actor_id(),
            &unrestricted,
            &document_projects,
            &year,
        ];
        let period = "created_at>=make_timestamptz($6,1,1,0,0,0,'UTC') AND created_at<make_timestamptz($6+1,1,1,0,0,0,'UTC')";
        let contributions: Vec<Value> = tx.query(&format!("WITH visible_tasks AS ({tasks}),events AS ({events}) SELECT jsonb_build_object('date',to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD'),'count',count(*)) FROM events WHERE {period} GROUP BY to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD') ORDER BY to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD')"),params).map_err(PlatformStore::database_error)?.iter().map(|row| row.get(0)).collect();
        let activity: Vec<Value> = tx.query(&format!("WITH visible_tasks AS ({tasks}),events AS ({events}) SELECT jsonb_build_object('id',id,'project_id',project_id,'kind',kind,'title',title,'created_at',created_at,'repository_id',(SELECT g.repository_id FROM git_commit_authorship g WHERE g.id=events.id),'repository_name',(SELECT COALESCE(r.remote_full_name,r.local_name,r.requested_full_name) FROM git_commit_authorship g JOIN project_repositories r ON r.id=g.repository_id WHERE g.id=events.id),'commit_sha',(SELECT g.commit_sha FROM git_commit_authorship g WHERE g.id=events.id),'branches',COALESCE((SELECT jsonb_agg(b.branch_name ORDER BY b.branch_name) FROM git_commit_branches b JOIN git_commit_authorship g ON g.repository_id=b.repository_id AND g.commit_sha=b.commit_sha WHERE g.id=events.id),'[]'::jsonb)) FROM events WHERE {period} ORDER BY created_at DESC,id DESC LIMIT 100"),params).map_err(PlatformStore::database_error)?.iter().map(|row| row.get(0)).collect();
        let analyses: i64 = tx.query_one("SELECT count(*) FROM repository_jobs WHERE repository_id=ANY($1) AND kind='index' AND state='succeeded'", &[&repository_ids]).map_err(PlatformStore::database_error)?.get(0);
        let synchronization: Value = tx.query_opt("SELECT jsonb_build_object('state',state,'updated_at',updated_at,'last_error',CASE WHEN state='failed' THEN 'Repository synchronization failed' ELSE NULL END) FROM repository_jobs WHERE repository_id=ANY($1) ORDER BY updated_at DESC,id DESC LIMIT 1", &[&repository_ids]).map_err(PlatformStore::database_error)?.map_or(json!({"state":null,"updated_at":null,"last_error":null}), |row| row.get(0));
        Ok(
            json!({"repository":null,"projects":projects,"synchronization":synchronization,"pull_requests":[],"pull_requests_available":false,"counts":{"tasks":task_count,"analyses":analyses,"pull_requests":0},"priority_tasks":priority_tasks,"contributions":contributions,"activity":activity,"year":year}),
        )
    }
}
