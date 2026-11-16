use crate::{
    PlatformStore, labor_calendar_policy::LaborCalendarPolicy,
    platform_authority::PlatformAuthority, task_read_authority::TaskReadAuthority,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};

pub(crate) struct OvertimeEvidenceStore;
impl OvertimeEvidenceStore {
    pub fn parse_task_ids(input: &Value) -> Result<Vec<String>, PlatformError> {
        Self::ids(input.get("task_ids"), true)
    }
    pub fn ids(input: Option<&Value>, required: bool) -> Result<Vec<String>, PlatformError> {
        let values = match input {
            Some(Value::Array(values)) => values,
            None if !required => return Ok(vec![]),
            _ => return Err(PlatformError::Validation("evidence IDs must be arrays")),
        };
        if values.len() > 100 || (required && values.is_empty()) {
            return Err(PlatformError::Validation(
                "one to one hundred planned tasks are required",
            ));
        }
        let ids = values
            .iter()
            .map(|v| {
                v.as_str()
                    .filter(|id| !id.is_empty() && id.len() <= 128)
                    .map(str::to_owned)
                    .ok_or(PlatformError::Validation("invalid evidence ID"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len() {
            return Err(PlatformError::Validation("duplicate evidence ID"));
        }
        Ok(ids)
    }
    pub fn bind_tasks(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        request: &str,
        ids: &[String],
    ) -> Result<(), PlatformError> {
        if ids.is_empty()
            || ids.len() > 100
            || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
            || ids.iter().any(|id| id.is_empty() || id.len() > 128)
        {
            return Err(PlatformError::Validation("planned tasks are required"));
        }
        let actor:String=tx.query_opt("SELECT actor_id,status FROM labor_overtime_requests WHERE id=$1 AND actor_id=$2 AND status IN ('pending','approved') FOR UPDATE",&[&request,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Forbidden)?.get(0);
        let approved: bool = tx
            .query_one(
                "SELECT status='approved' FROM labor_overtime_requests WHERE id=$1",
                &[&request],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        let existing_projects = Self::request_projects(tx, request)?;
        for id in ids {
            let project:String=tx.query_opt("SELECT t.project_id FROM tasks t JOIN projects p ON p.id=t.project_id WHERE t.id=$1 AND NOT t.archived AND NOT p.archived AND (t.assignee_id=$2 OR EXISTS(SELECT 1 FROM task_history h WHERE h.task_id=t.id AND (h.details->'before'->>'assignee_id'=$2 OR h.details->'after'->>'assignee_id'=$2 OR (h.actor_id=$2 AND h.operation='task.transitioned')))) FOR SHARE OF t,p",&[&id,&actor]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
            if approved && !existing_projects.contains(&project) {
                return Err(PlatformError::Validation(
                    "approved overtime evidence must remain within its original task projects",
                ));
            }
            if !Self::target_project(tx, &actor, &project, Capability::TasksRead)? {
                return Err(PlatformError::Forbidden);
            }
            TaskReadAuthority::require_task(tx, ctx, &project, id)?;
            tx.execute("INSERT INTO overtime_task_links(request_id,task_id,added_by) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",&[&request,&id,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        }
        Ok(())
    }
    pub fn request_projects(
        tx: &mut Transaction<'_>,
        request: &str,
    ) -> Result<Vec<String>, PlatformError> {
        Ok(tx.query("SELECT t.project_id FROM overtime_task_links l JOIN tasks t ON t.id=l.task_id WHERE l.request_id=$1 UNION SELECT r.project_id FROM overtime_commit_links l JOIN repository_source_events e ON e.id=l.source_event_id JOIN project_repositories r ON r.id=e.repository_id WHERE l.request_id=$1 ORDER BY 1",&[&request]).map_err(PlatformStore::database_error)?.iter().map(|r|r.get(0)).collect())
    }
    fn target_project(
        tx: &mut Transaction<'_>,
        actor: &str,
        project: &str,
        capability: Capability,
    ) -> Result<bool, PlatformError> {
        Ok(tx.query_one("SELECT EXISTS(SELECT 1 FROM actor_roles ar JOIN actors a ON a.id=ar.actor_id WHERE a.id=$1 AND a.active AND ar.role IN ('developer','technical_lead','pmo','manager','super_admin') AND EXISTS(SELECT 1 FROM projects p WHERE p.id=$2 AND NOT p.archived) AND (ar.all_projects OR EXISTS(SELECT 1 FROM member_role_projects mp WHERE mp.actor_id=a.id AND mp.project_id=$2)) AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=a.id AND g.capability=$3 AND (g.project_id IS NULL OR g.project_id=$2)))",&[&actor,&project,&capability.as_str()]).map_err(PlatformStore::database_error)?.get(0))
    }
    pub fn planned_tasks(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
    ) -> Result<Vec<Value>, PlatformError> {
        let scope = TaskReadAuthority::scope(tx, ctx)?;
        let rows=tx.query("SELECT t.id,t.project_id,t.title FROM tasks t JOIN projects p ON p.id=t.project_id WHERE t.project_id=ANY($2) AND EXISTS(SELECT 1 FROM actor_roles ar WHERE ar.actor_id=$1 AND ar.role IN ('developer','technical_lead','pmo','manager','super_admin') AND (ar.all_projects OR EXISTS(SELECT 1 FROM member_role_projects mp WHERE mp.actor_id=$1 AND mp.project_id=t.project_id))) AND NOT t.archived AND NOT p.archived AND (t.assignee_id=$1 OR EXISTS(SELECT 1 FROM task_history h WHERE h.task_id=t.id AND (h.details->'before'->>'assignee_id'=$1 OR h.details->'after'->>'assignee_id'=$1 OR (h.actor_id=$1 AND h.operation='task.transitioned')))) AND (t.assignee_id=$1 OR t.created_by=$1 OR t.project_id=ANY($3) OR EXISTS(SELECT 1 FROM task_history h WHERE h.task_id=t.id AND h.actor_id=$1 AND h.operation IN ('task.created','task.changed','task.reassigned_workflow') AND h.details->'after'->>'assignee_id' IS NOT NULL AND (h.details->'before'->>'assignee_id') IS DISTINCT FROM (h.details->'after'->>'assignee_id'))) ORDER BY t.id LIMIT 501",&[&ctx.actor_id(),&scope.projects,&scope.unrestricted]).map_err(PlatformStore::database_error)?;
        Ok(rows.iter().map(|r|json!({"id":r.get::<_,String>(0),"project_id":r.get::<_,String>(1),"title":r.get::<_,String>(2)})).collect())
    }
    pub fn candidates(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        request: &str,
    ) -> Result<Value, PlatformError> {
        let row=tx.query_opt("SELECT actor_id,status FROM labor_overtime_requests WHERE id=$1 AND actor_id=$2 FOR SHARE",&[&request,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, String>(1) == "rejected" {
            return Ok(json!({"tasks":[],"commits":[],"can_attach_evidence":false}));
        }
        if Self::projection(tx, ctx, request)?["can_attach_evidence"] != true {
            return Ok(json!({"tasks":[],"commits":[],"can_attach_evidence":false}));
        }
        let tasks = Self::planned_tasks(tx, ctx)?;
        let commits = Self::verified_commits(tx, ctx, request, None)?;
        Ok(json!({"tasks":tasks,"commits":commits,"can_attach_evidence":true}))
    }
    pub fn verified_commits(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        request: &str,
        ids: Option<&Vec<String>>,
    ) -> Result<Vec<Value>, PlatformError> {
        let rows=tx.query("SELECT DISTINCT e.id,r.id,r.project_id,COALESCE(r.remote_full_name,r.requested_full_name),e.source_commit,p.occurred_at::text FROM labor_overtime_requests o JOIN developer_push_activity p ON p.actor_id=o.actor_id JOIN repository_source_events e ON e.provider=p.provider AND e.scope_id=p.scope_id AND e.delivery_id=p.delivery_id JOIN project_repositories r ON r.id=e.repository_id AND r.project_id=p.project_id JOIN projects project ON project.id=r.project_id WHERE o.id=$1 AND ($2::text[] IS NULL OR e.id=ANY($2)) AND o.actor_id=$3 AND NOT r.archived AND NOT project.archived AND r.authorization_status='authorized' AND p.occurred_at>=((o.date+o.start_time::time) AT TIME ZONE o.timezone) AND p.occurred_at<((o.date+o.end_time::time+CASE WHEN o.next_day THEN interval '1 day' ELSE interval '0 day' END) AT TIME ZONE o.timezone) AND EXISTS(SELECT 1 FROM overtime_task_links l JOIN tasks t ON t.id=l.task_id WHERE l.request_id=o.id AND t.project_id=r.project_id) ORDER BY e.id LIMIT 501",&[&request,&ids,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        let mut commits = vec![];
        for row in rows {
            let project: String = row.get(2);
            if !Self::target_project(tx, ctx.actor_id(), &project, Capability::RepositoriesRead)? {
                continue;
            }
            let mut task_visible = false;
            if Self::target_project(tx, ctx.actor_id(), &project, Capability::TasksRead)? {
                for task in tx.query("SELECT t.id FROM overtime_task_links l JOIN tasks t ON t.id=l.task_id WHERE l.request_id=$1 AND t.project_id=$2",&[&request,&project]).map_err(PlatformStore::database_error)? {
                    let task_id:String=task.get(0);
                    match TaskReadAuthority::require_task(tx,ctx,&project,&task_id) {Ok(())=>{task_visible=true;break;},Err(PlatformError::Forbidden|PlatformError::NotFound)=>{},Err(e)=>return Err(e)};
                }
            }
            if !task_visible {
                continue;
            }

            match PlatformAuthority::require(tx, ctx, Capability::RepositoriesRead, Some(&project))
            {
                Ok(()) => {}
                Err(PlatformError::Forbidden) => continue,
                Err(e) => return Err(e),
            };
            commits.push(json!({"source_event_id":row.get::<_,String>(0),"repository_id":row.get::<_,String>(1),"project_id":project,"repository_name":row.get::<_,String>(3),"sha":row.get::<_,String>(4),"occurred_at":row.get::<_,String>(5),"provenance":"verified_push_receipt"}));
        }
        Ok(commits)
    }
    pub fn projection(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        request: &str,
    ) -> Result<Value, PlatformError> {
        let row = tx
            .query_one(
                "SELECT actor_id,status FROM labor_overtime_requests WHERE id=$1",
                &[&request],
            )
            .map_err(PlatformStore::database_error)?;
        let actor: String = row.get(0);
        let mut tasks = vec![];
        let mut unavailable = 0usize;
        let mut all_visible = true;
        for row in tx.query("SELECT t.id,t.project_id,t.title FROM overtime_task_links l JOIN tasks t ON t.id=l.task_id WHERE l.request_id=$1 ORDER BY t.id",&[&request]).map_err(PlatformStore::database_error)? {
            let id:String=row.get(0);let project:String=row.get(1);
            if !Self::target_project(tx,&actor,&project,Capability::TasksRead)? {all_visible=false;unavailable+=1;continue;}
            match TaskReadAuthority::require_task(tx,ctx,&project,&id) {Ok(())=>tasks.push(json!({"id":id,"project_id":project,"title":row.get::<_,String>(2)})),Err(PlatformError::Forbidden|PlatformError::NotFound)=>{all_visible=false;unavailable+=1;},Err(e)=>return Err(e)};
        }
        let mut commits = vec![];
        for row in tx.query("SELECT e.id,r.id,r.project_id,COALESCE(r.remote_full_name,r.requested_full_name),e.source_commit,(SELECT min(p.occurred_at)::text FROM developer_push_activity p WHERE p.actor_id=$2 AND p.provider=e.provider AND p.scope_id=e.scope_id AND p.delivery_id=e.delivery_id AND p.project_id=r.project_id) FROM overtime_commit_links l JOIN repository_source_events e ON e.id=l.source_event_id JOIN project_repositories r ON r.id=e.repository_id WHERE l.request_id=$1 ORDER BY e.id",&[&request,&actor]).map_err(PlatformStore::database_error)? {
            let project:String=row.get(2);
            if !Self::target_project(tx,&actor,&project,Capability::RepositoriesRead)? || !tasks.iter().any(|t|t["project_id"]==project){unavailable+=1;continue;}
            match PlatformAuthority::require(tx,ctx,Capability::RepositoriesRead,Some(&project)) {Ok(())=>{},Err(PlatformError::Forbidden)=>{unavailable+=1;continue;},Err(e)=>return Err(e)};
            commits.push(json!({"source_event_id":row.get::<_,String>(0),"repository_id":row.get::<_,String>(1),"project_id":project,"repository_name":row.get::<_,String>(3),"sha":row.get::<_,String>(4),"occurred_at":row.get::<_,Option<String>>(5),"provenance":"verified_push_receipt"}));
        }
        let developer = LaborCalendarPolicy::member(tx, ctx.actor_id()).is_ok();
        Ok(
            json!({"tasks":tasks,"commits":commits,"evidence_unavailable":unavailable>0,"evidence_unavailable_count":unavailable,"can_attach_evidence":ctx.actor_id()==actor && developer && row.get::<_,String>(1)!="rejected" && all_visible}),
        )
    }
}
