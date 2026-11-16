use crate::{
    PlatformStore, platform_authority::PlatformAuthority, repository_access::RepositoryAccess,
    repository_job_source::RepositoryJobSource, task_read_authority::TaskReadAuthority,
    task_read_predicate::task_read_predicate,
};
use nexora_platform::{AuthContext, Capability, PlatformError, RepositorySourceKind};
use postgres::Transaction;
use serde_json::{Value, json};

pub(crate) struct MemberProfileStore;
impl MemberProfileStore {
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        member: &str,
        year: Option<i32>,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        // All signed-in team members may read profiles; each data section still
        // intersects the viewer's project permissions and excludes private feedback.
        crate::labor_calendar_policy::LaborCalendarPolicy::session(&mut tx, ctx)?;
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
        let identity: Value = tx.query_opt(&format!("SELECT {} FROM actors a JOIN users u ON u.actor_id=a.id LEFT JOIN notification_recipients r ON r.actor_id=a.id WHERE a.id=$1 AND a.kind='human' AND (a.id=$2 OR NOT EXISTS(SELECT 1 FROM actor_roles ar WHERE ar.actor_id=a.id AND ar.role='super_admin')) FOR SHARE OF a,u", crate::member_public_projection::member_public_projection()), &[&member,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        // Lock target grants so removal cannot race this response's project intersection.
        let grants = tx
            .query(
                "SELECT capability,project_id FROM actor_grants WHERE actor_id=$1 FOR SHARE",
                &[&member],
            )
            .map_err(PlatformStore::database_error)?;
        let target_allows = |capability: Capability, project: &str| {
            grants.iter().any(|row| {
                row.get::<_, String>(0) == capability.as_str()
                    && row
                        .get::<_, Option<String>>(1)
                        .as_deref()
                        .is_none_or(|id| id == project)
            })
        };
        let candidates = tx
            .query(
                "SELECT id,name FROM projects WHERE NOT archived ORDER BY name,id",
                &[],
            )
            .map_err(PlatformStore::database_error)?;
        let mut projects = Vec::new();
        let mut repository_projects = Vec::new();
        let mut authorship_projects = Vec::new();
        let mut task_projects = Vec::new();
        let mut unrestricted_projects = Vec::new();
        let mut document_projects = Vec::new();
        for row in candidates {
            let project: String = row.get(0);
            // Historical authorship survives changes to the member's current project grants.
            // Only the viewer's live repository access governs disclosure of these commits.
            if Self::allows(&mut tx, ctx, Capability::RepositoriesRead, &project)? {
                authorship_projects.push(project.clone());
            }
            let mut visible = false;
            for capability in [
                Capability::ProjectsRead,
                Capability::RepositoriesRead,
                Capability::TasksRead,
                Capability::DocumentsRead,
            ] {
                if !target_allows(capability, &project)
                    || !Self::allows(&mut tx, ctx, capability, &project)?
                {
                    continue;
                }
                visible = true;
                match capability {
                    Capability::RepositoriesRead => repository_projects.push(project.clone()),
                    Capability::TasksRead => {
                        task_projects.push(project.clone());
                        if TaskReadAuthority::unrestricted(&mut tx, ctx, &project)? {
                            unrestricted_projects.push(project.clone());
                        }
                    }
                    Capability::DocumentsRead => document_projects.push(project.clone()),
                    _ => {}
                }
            }
            if visible {
                PlatformAuthority::project(&mut tx, &project)?;
                projects.push(json!({"id":project,"name":row.get::<_,String>(1)}));
            }
        }
        let repository_rows = tx.query("SELECT id,project_id,COALESCE(remote_full_name,local_name,requested_full_name),provider,authorization_status FROM project_repositories WHERE project_id=ANY($1) AND NOT archived AND authorization_status='authorized' ORDER BY updated_at DESC,id", &[&repository_projects]).map_err(PlatformStore::database_error)?;
        let mut repositories = Vec::new();
        for row in repository_rows {
            if repositories.len() == 6 {
                break;
            }
            let repository: String = row.get(0);
            let project: String = row.get(1);
            let access = match RepositoryAccess::lock(&mut tx, ctx, &project, &repository) {
                Ok(access) => access,
                Err(PlatformError::NotFound | PlatformError::Forbidden) => continue,
                Err(error) => return Err(error),
            };
            if access.source_kind == RepositorySourceKind::Git {
                match RepositoryJobSource::lock(
                    &mut tx,
                    ctx,
                    &project,
                    &repository,
                    access.revision,
                ) {
                    Ok(_) => {}
                    Err(
                        PlatformError::NotFound
                        | PlatformError::Forbidden
                        | PlatformError::Conflict(_),
                    ) => continue,
                    Err(error) => return Err(error),
                }
            } else {
                // Local authorization/archiving may change independently of grants.
                if tx.query_opt("SELECT id FROM project_repositories WHERE id=$1 AND authorization_status='authorized' AND NOT archived FOR SHARE", &[&repository]).map_err(PlatformStore::database_error)?.is_none() { continue; }
            }
            repositories.push(json!({"id":repository,"project_id":project,"name":row.get::<_,String>(2),"provider":row.get::<_,Option<String>>(3),"status":row.get::<_,String>(4)}));
        }
        let predicate = task_read_predicate("$3", "t.project_id=ANY($4)");
        let priority_tasks: Vec<Value> = tx.query(&format!("SELECT jsonb_build_object('id',t.id,'project_id',t.project_id,'title',t.title,'priority',t.priority) FROM tasks t WHERE t.project_id=ANY($1) AND (t.assignee_id=$2 OR t.created_by=$2) AND NOT t.archived AND t.priority IN ('high','critical') AND NOT EXISTS(SELECT 1 FROM workflow_versions w CROSS JOIN LATERAL jsonb_array_elements(w.graph->'states') s WHERE w.id=t.workflow_version_id AND s->>'id'=t.state_id AND s->>'terminal'='true') AND {predicate} ORDER BY CASE t.priority WHEN 'critical' THEN 0 ELSE 1 END,t.updated_at DESC,t.id LIMIT 8"), &[&task_projects,&member,&ctx.actor_id(),&unrestricted_projects]).map_err(PlatformStore::database_error)?.iter().map(|row| row.get(0)).collect();
        let active_stage = crate::task_active_stage_predicate::TASK_ACTIVE_STAGE_PREDICATE;
        let workflow_tasks: Vec<Value> = tx.query(&format!("SELECT jsonb_build_object('id',t.id,'project_id',t.project_id,'title',t.title,'state',s->>'name','workflow',w.name,'color',s->>'color','priority',t.priority) FROM tasks t JOIN workflows w ON w.id=t.workflow_id JOIN workflow_versions v ON v.id=t.workflow_version_id CROSS JOIN LATERAL jsonb_array_elements(v.graph->'states') s WHERE t.assignee_id=$2 AND t.lifecycle='active' AND NOT t.archived AND {active_stage} AND s->>'id'=t.state_id AND (t.project_id IS NULL OR (t.project_id=ANY($1) AND {predicate})) ORDER BY t.title,t.id"), &[&task_projects,&member,&ctx.actor_id(),&unrestricted_projects]).map_err(PlatformStore::database_error)?.into_iter().map(|row|row.get(0)).collect();
        // Both chart and feed derive from the exact same visible immutable events.
        let commit_events = "SELECT DISTINCT ON (g.provider,g.host,g.remote_repository_id,g.commit_sha) g.id,g.project_id,'commit'::text AS kind,COALESCE(NULLIF(g.commit_message,''),g.commit_sha) AS title,g.occurred_at AS created_at FROM git_commit_authorship g JOIN project_repositories repository ON repository.id=g.repository_id JOIN git_connections connection ON connection.id=repository.connection_id JOIN actors owner ON owner.id=connection.owner_actor_id AND owner.active WHERE g.actor_id=$2 AND g.project_id=ANY($7) AND NOT repository.archived AND repository.authorization_status='authorized' AND connection.status='active' ORDER BY g.provider,g.host,g.remote_repository_id,g.commit_sha,g.occurred_at,g.id";
        let events = format!(
            "SELECT h.id,h.project_id,'task'::text AS kind,t.title,h.created_at FROM task_history h JOIN tasks t ON t.id=h.task_id AND t.project_id=h.project_id WHERE h.actor_id=$2 AND h.project_id=ANY($1) AND {predicate} UNION ALL SELECT v.id,v.project_id,'document'::text,v.title,v.created_at FROM project_document_versions v WHERE v.created_by=$2 AND v.project_id=ANY($5) UNION ALL SELECT * FROM ({commit_events}) git_events"
        );
        let parameters: &[&(dyn postgres::types::ToSql + Sync)] = &[
            &task_projects,
            &member,
            &ctx.actor_id(),
            &unrestricted_projects,
            &document_projects,
            &year,
            &authorship_projects,
        ];
        // Management profiles are not measured through contribution charts.
        // Enforce this at the API boundary, regardless of the viewer's role.
        let management = matches!(identity["role"].as_str(), Some("manager" | "super_admin"));
        let contributions: Vec<Value> = if management {
            Vec::new()
        } else {
            tx.query(&format!("WITH events AS ({events}) SELECT jsonb_build_object('date',to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD'),'count',count(*)) FROM events WHERE created_at>=make_timestamptz($6,1,1,0,0,0,'UTC') AND created_at<make_timestamptz($6+1,1,1,0,0,0,'UTC') GROUP BY to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD') ORDER BY to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD')"), parameters).map_err(PlatformStore::database_error)?.iter().map(|row| row.get(0)).collect()
        };
        let activity: Vec<Value> = tx.query(&format!("WITH events AS ({events}), recent AS (SELECT * FROM events WHERE created_at>=make_timestamptz($6,1,1,0,0,0,'UTC') AND created_at<make_timestamptz($6+1,1,1,0,0,0,'UTC') ORDER BY created_at DESC,id DESC LIMIT 100) SELECT jsonb_build_object('id',id,'project_id',project_id,'kind',kind,'title',title,'created_at',created_at,'repository_id',(SELECT g.repository_id FROM git_commit_authorship g WHERE g.id=recent.id),'repository_name',(SELECT COALESCE(r.remote_full_name,r.local_name,r.requested_full_name) FROM git_commit_authorship g JOIN project_repositories r ON r.id=g.repository_id WHERE g.id=recent.id),'commit_sha',(SELECT g.commit_sha FROM git_commit_authorship g WHERE g.id=recent.id),'branches',(SELECT COALESCE(jsonb_agg(b.branch_name ORDER BY b.branch_name),'[]'::jsonb) FROM git_commit_branches b JOIN git_commit_authorship g ON g.repository_id=b.repository_id AND g.commit_sha=b.commit_sha WHERE g.id=recent.id)) FROM recent ORDER BY created_at,id"), parameters).map_err(PlatformStore::database_error)?.iter().map(|row| row.get(0)).collect();
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(
            json!({"member":identity,"projects":projects,"repositories":repositories,"workflow_tasks":workflow_tasks,"priority_tasks":priority_tasks,"contributions":contributions,"activity":activity,"year":year}),
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
