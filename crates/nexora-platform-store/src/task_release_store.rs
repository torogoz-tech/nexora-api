use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
    task_history_writer::TaskHistoryWriter, task_policy_store::TaskPolicyStore,
    task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};
pub(crate) struct TaskReleaseStore;
impl TaskReleaseStore {
    fn authority(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        write: bool,
    ) -> Result<(), PlatformError> {
        PlatformAuthority::require(
            tx,
            ctx,
            if write {
                Capability::TasksWrite
            } else {
                Capability::TasksRead
            },
            Some(project),
        )?;
        PlatformAuthority::project(tx, project)?;
        if write {
            tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('technical_lead','manager','super_admin') FOR SHARE",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Forbidden)?;
        }
        Ok(())
    }
    pub(crate) fn authorize_publication(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<(), PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authority(&mut tx, ctx, project, true)?;
        tx.commit().map_err(PlatformStore::database_error)
    }
    pub(crate) fn validate_pins(
        store: &mut PlatformStore,
        project: &str,
        release: &str,
    ) -> Result<(), PlatformError> {
        let invalid:bool=store.client.query_one("SELECT EXISTS(SELECT 1 FROM task_release_items i LEFT JOIN tasks t ON t.id=i.task_id WHERE i.release_id=$1 AND (t.id IS NULL OR t.project_id IS DISTINCT FROM $2 OR t.revision<>i.task_revision OR t.archived OR t.lifecycle<>'active')) OR EXISTS(SELECT 1 FROM task_release_commit_pins p LEFT JOIN task_commit_links l ON l.task_id=p.task_id AND l.repository_id=p.repository_id AND l.commit_sha=p.commit_sha AND l.removed_at IS NULL WHERE p.release_id=$1 AND (l.id IS NULL OR l.provider_evidence->>'provenance' IS DISTINCT FROM 'provider_verified'))",&[&release,&project]).map_err(PlatformStore::database_error)?.get(0);
        if invalid {
            return Err(PlatformError::Conflict(
                "selected release task revisions or commit bindings changed",
            ));
        }
        Ok(())
    }
    fn projection(
        tx: &mut Transaction<'_>,
        project: &str,
        id: &str,
    ) -> Result<Value, PlatformError> {
        Ok(tx.query_opt("SELECT to_jsonb(r)||jsonb_build_object('tasks',COALESCE((SELECT jsonb_agg(to_jsonb(i) ORDER BY task_id) FROM task_release_items i WHERE release_id=r.id),'[]'::jsonb),'commits',COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY task_id,sequence,repository_id,commit_sha) FROM task_release_commit_pins c WHERE release_id=r.id),'[]'::jsonb),'sources',COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY task_id,repository_id) FROM task_release_source_pins s WHERE release_id=r.id),'[]'::jsonb),'repositories',COALESCE((SELECT jsonb_agg(to_jsonb(i) ORDER BY repository_id) FROM task_release_repositories i WHERE release_id=r.id),'[]'::jsonb)) FROM task_releases r WHERE id=$1 AND project_id=$2",&[&id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0))
    }
    pub fn list(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authority(&mut tx, ctx, project, false)?;
        let values:Vec<Value>=tx.query("SELECT to_jsonb(r) FROM task_releases r WHERE project_id=$1 ORDER BY created_at DESC,id LIMIT 100",&[&project]).map_err(PlatformStore::database_error)?.into_iter().map(|r|r.get(0)).collect();
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(json!({"items":values}))
    }
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authority(&mut tx, ctx, project, false)?;
        let value = Self::projection(&mut tx, project, id)?;
        for task in value["tasks"].as_array().ok_or(PlatformError::Storage)? {
            TaskScopeAuthority::task(
                &mut tx,
                ctx,
                task["task_id"].as_str().ok_or(PlatformError::Storage)?,
                Capability::TasksRead,
            )?;
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
    pub fn save(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: Option<&str>,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let allowed = if id.is_some() {
            vec!["expected_revision", "tasks", "repositories"]
        } else {
            vec!["version", "branch_name", "tasks", "repositories"]
        };
        if input
            .as_object()
            .is_none_or(|values| values.keys().any(|key| !allowed.contains(&key.as_str())))
        {
            return Err(PlatformError::Validation("unsupported release input field"));
        }
        let operation = if id.is_some() {
            "release.tasks_add"
        } else {
            "release.create"
        };
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_relation_lock::TaskRelationLock::acquire(&mut tx)?;
        Self::authority(&mut tx, ctx, project, true)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            operation,
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(value);
        }
        let generated = SecretGenerator::id("rel")?;
        let release = if let Some(id) = id {
            if tx.query_one("SELECT EXISTS(SELECT 1 FROM task_release_composition_jobs WHERE release_id=$1 AND status IN ('queued','running','succeeded'))",&[&id]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("release selection is frozen once composition is queued; prepare another release for additional changes"));}

            let row=tx.query_opt("SELECT revision,status,created_by,publication_evidence FROM task_releases WHERE id=$1 AND project_id=$2 FOR UPDATE",&[&id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
            if row.get::<_, i64>(0)
                != input["expected_revision"]
                    .as_i64()
                    .ok_or(PlatformError::Validation(
                        "release expected_revision required",
                    ))?
                || row.get::<_, String>(1) != "prepared"
            {
                return Err(PlatformError::Conflict(
                    "release changed or is already published",
                ));
            }
            id
        } else {
            let version = DomainText::required(
                input["version"]
                    .as_str()
                    .ok_or(PlatformError::Validation("release version required"))?,
                128,
            )?;
            let branch = DomainText::required(
                input["branch_name"]
                    .as_str()
                    .ok_or(PlatformError::Validation("release branch required"))?,
                200,
            )?;
            if !branch.starts_with("release/")
                || branch.contains("..")
                || branch
                    .chars()
                    .any(|c| c.is_whitespace() || "~^:?*[\\".contains(c))
            {
                return Err(PlatformError::Validation(
                    "release branch must use a valid release/ name",
                ));
            }
            tx.execute("INSERT INTO task_releases(id,project_id,version,branch_name,created_by) VALUES($1,$2,$3,$4,$5)",&[&generated,&project,&version,&branch,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
            &generated
        };
        let tasks = input["tasks"]
            .as_array()
            .ok_or(PlatformError::Validation("explicit release tasks required"))?;
        if tasks.is_empty() || tasks.len() > 100 {
            return Err(PlatformError::Validation(
                "select between 1 and 100 release tasks",
            ));
        }
        for selection in tasks {
            let task = selection["task_id"]
                .as_str()
                .ok_or(PlatformError::Validation("release task required"))?;
            let revision = selection["expected_revision"]
                .as_i64()
                .ok_or(PlatformError::Validation("task expected_revision required"))?;
            let scoped = TaskScopeAuthority::task(&mut tx, ctx, task, Capability::TasksWrite)?;
            if scoped.project_id.as_deref() != Some(project) {
                return Err(PlatformError::NotFound);
            }
            let current = TaskPolicyStore::scoped_task(&mut tx, Some(project), task)?;
            if current.revision != revision
                || current.archived
                || current.lifecycle != TaskLifecycle::Active
                || !current.governed_stage
                || current.workflow_version_id.is_none()
            {
                return Err(PlatformError::Conflict(
                    "release task must be an unchanged active governed workflow task",
                ));
            }
            if !tx.query_one("SELECT EXISTS(SELECT 1 FROM workflow_versions v,jsonb_array_elements(v.graph->'states') s WHERE v.id=$1 AND s->>'id'=$2 AND COALESCE((s->>'terminal')::boolean,false))",&[&current.workflow_version_id,&current.state_id]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("finish task stage before selecting it for release"));}
            crate::task_relation_policy::TaskRelationPolicy::take(&mut tx, &current)?;
            crate::task_relation_policy::TaskRelationPolicy::state(&mut tx, &current, true)?;
            if tx.query_one("SELECT EXISTS(SELECT 1 FROM task_subtasks WHERE child_task_id=$1 AND removed_at IS NULL AND work_kind='decomposition')",&[&task]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("decomposition children deliver through their parent release"));}
            tx.execute(
                "INSERT INTO task_release_items(release_id,task_id,task_revision) VALUES($1,$2,$3)",
                &[&release, &task, &revision],
            )
            .map_err(PlatformStore::database_error)?;
        }
        let descendants=tx.query("WITH RECURSIVE included(task_id,parent_id) AS (SELECT s.child_task_id,s.parent_task_id FROM task_subtasks s JOIN task_release_items i ON i.task_id=s.parent_task_id AND i.release_id=$1 WHERE s.removed_at IS NULL AND s.work_kind='decomposition' UNION SELECT s.child_task_id,s.parent_task_id FROM task_subtasks s JOIN included p ON p.task_id=s.parent_task_id WHERE s.removed_at IS NULL AND s.work_kind='decomposition') SELECT task_id,parent_id FROM included ORDER BY task_id LIMIT 1001",&[&release]).map_err(PlatformStore::database_error)?;
        if descendants.len() > 1000 {
            return Err(PlatformError::Validation(
                "release decomposition exceeds 1000 subtasks",
            ));
        }
        for descendant in descendants {
            let task: String = descendant.get(0);
            let parent: String = descendant.get(1);
            let current = TaskScopeAuthority::task(&mut tx, ctx, &task, Capability::TasksWrite)?;
            if current.lifecycle == TaskLifecycle::Completed {
                continue;
            }
            if current.project_id.as_deref() != Some(project)
                || current.archived
                || current.lifecycle != TaskLifecycle::Active
                || !current.governed_stage
                || current.workflow_version_id.is_none()
            {
                return Err(PlatformError::Conflict(
                    "integrated decomposition child must have an unchanged active finished stage",
                ));
            }
            let terminal:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM workflow_versions v,jsonb_array_elements(v.graph->'states') s WHERE v.id=$1 AND s->>'id'=$2 AND COALESCE((s->>'terminal')::boolean,false))",&[&current.workflow_version_id,&current.state_id]).map_err(PlatformStore::database_error)?.get(0);
            if !terminal {
                return Err(PlatformError::Conflict(
                    "finish decomposition child stages before release selection",
                ));
            }
            crate::task_relation_policy::TaskRelationPolicy::state(&mut tx, &current, true)?;
            tx.execute("INSERT INTO task_release_items(release_id,task_id,task_revision,included_by_parent_id) VALUES($1,$2,$3,$4) ON CONFLICT(release_id,task_id) DO NOTHING",&[&release,&task,&current.revision,&parent]).map_err(PlatformStore::database_error)?;
        }
        let repositories = input["repositories"]
            .as_array()
            .ok_or(PlatformError::Validation(
                "explicit release repositories required",
            ))?;
        if repositories.is_empty() || repositories.len() > 8 {
            return Err(PlatformError::Validation(
                "select between 1 and 8 release repositories",
            ));
        }
        for selection in repositories {
            let repository = selection["repository_id"]
                .as_str()
                .ok_or(PlatformError::Validation("release repository required"))?;
            let sha = selection["head_sha"].as_str();
            if sha.is_some_and(|sha| {
                ![40, 64].contains(&sha.len()) || !sha.bytes().all(|b| b.is_ascii_hexdigit())
            }) {
                return Err(PlatformError::Validation(
                    "release head SHA must be a full commit hash",
                ));
            }
            PlatformAuthority::require(&mut tx, ctx, Capability::RepositoriesRead, Some(project))?;
            tx.query_opt("SELECT id FROM project_repositories WHERE id=$1 AND project_id=$2 AND NOT archived AND authorization_status='authorized' FOR SHARE",&[&repository,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
            tx.execute("INSERT INTO task_release_repositories(release_id,repository_id,head_sha) VALUES($1,$2,$3) ON CONFLICT(release_id,repository_id) DO NOTHING",&[&release,&repository,&sha]).map_err(PlatformStore::database_error)?;
            let pinned:Option<String>=tx.query_one("SELECT head_sha FROM task_release_repositories WHERE release_id=$1 AND repository_id=$2",&[&release,&repository]).map_err(PlatformStore::database_error)?.get(0);
            if sha.is_some() && pinned.as_deref() != sha {
                return Err(PlatformError::Conflict(
                    "release repository head is already pinned",
                ));
            }
        }
        let source_tasks = tx
            .query(
                "SELECT task_id FROM task_release_items WHERE release_id=$1 ORDER BY task_id",
                &[&release],
            )
            .map_err(PlatformStore::database_error)?;
        for selection in source_tasks {
            let task: String = selection.get(0);
            let existing:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_release_source_pins WHERE release_id=$1 AND task_id=$2)",&[&release,&task]).map_err(PlatformStore::database_error)?.get(0);
            if existing {
                continue;
            }
            let branches=tx.query("SELECT repository_id,head_sha FROM task_repository_branches WHERE task_id=$1 ORDER BY repository_id",&[&task]).map_err(PlatformStore::database_error)?;
            if branches.is_empty() {
                return Err(PlatformError::Conflict(
                    "release task requires provider-recorded repository branch heads",
                ));
            }
            for branch in branches {
                let repository: String = branch.get(0);
                let sha: String = branch.get(1);
                if !tx.query_one("SELECT EXISTS(SELECT 1 FROM task_release_repositories WHERE release_id=$1 AND repository_id=$2)",&[&release,&repository]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){return Err(PlatformError::Validation("release must select every repository used by its tasks"));}
                tx.execute("INSERT INTO task_release_source_pins(release_id,task_id,repository_id,source_head_sha) VALUES($1,$2,$3,$4)",&[&release,&task,&repository,&sha]).map_err(PlatformStore::database_error)?;
            }
        }
        let items = tx
            .query(
                "SELECT task_id FROM task_release_items WHERE release_id=$1 ORDER BY task_id",
                &[&release],
            )
            .map_err(PlatformStore::database_error)?;
        for item in items {
            let task: String = item.get(0);
            let refs=tx.query("SELECT repository_id,commit_sha,sequence FROM task_commit_links WHERE task_id=$1 AND removed_at IS NULL ORDER BY sequence,id FOR SHARE",&[&task]).map_err(PlatformStore::database_error)?;
            if refs.is_empty() {
                return Err(PlatformError::Conflict(
                    "bind explicit provider-verified task commits before preparing a release",
                ));
            }
            for reference in refs {
                let repository: String = reference.get(0);
                let sha: String = reference.get(1);
                let sequence: i64 = reference.get(2);
                if !tx.query_one("SELECT EXISTS(SELECT 1 FROM task_release_repositories WHERE release_id=$1 AND repository_id=$2)",&[&release,&repository]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){return Err(PlatformError::Validation("select every repository referenced by task commits"));}
                tx.execute("INSERT INTO task_release_commit_pins(release_id,task_id,repository_id,commit_sha,sequence) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",&[&release,&task,&repository,&sha,&sequence]).map_err(PlatformStore::database_error)?;
            }
        }
        if id.is_some() {
            tx.execute(
                "UPDATE task_releases SET revision=revision+1 WHERE id=$1",
                &[&release],
            )
            .map_err(PlatformStore::database_error)?;
        }
        let value = Self::projection(&mut tx, project, release)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            operation,
            Some(release),
            json!({"project_id":project,"release":value}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
    /// Called only after the transition executor independently verified production deployments.
    pub(crate) fn publish(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        revision: i64,
        verified_repositories: &[(String, String)],
        evidence: &Value,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_relation_lock::TaskRelationLock::acquire(&mut tx)?;
        Self::authority(&mut tx, ctx, project, true)?;
        let row=tx.query_opt("SELECT revision,status,created_by,publication_evidence FROM task_releases WHERE id=$1 AND project_id=$2 FOR UPDATE",&[&id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, String>(1) == "published"
            && row.get::<_, i64>(0) == revision + 1
            && row.get::<_, Option<Value>>(3).is_some_and(|old| {
                old["transition_run_id"].is_string()
                    && old["transition_run_id"] == evidence["transition_run_id"]
            })
        {
            let value = Self::projection(&mut tx, project, id)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(value);
        }
        if row.get::<_, i64>(0) != revision || row.get::<_, String>(1) != "prepared" {
            return Err(PlatformError::Conflict(
                "release changed before production publication",
            ));
        }
        if tx.query_one("SELECT EXISTS(SELECT 1 FROM task_release_repositories WHERE release_id=$1 AND (composition_status<>'composed' OR head_sha IS NULL OR composition_evidence->>'provenance' IS DISTINCT FROM 'server_composed'))",&[&id]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("compose and verify the selected release commits before publication"));}
        let expected:Vec<(String,String)>=tx.query("SELECT repository_id,head_sha FROM task_release_repositories WHERE release_id=$1 ORDER BY repository_id",&[&id]).map_err(PlatformStore::database_error)?.into_iter().map(|r|(r.get(0),r.get(1))).collect();
        let mut verified = verified_repositories.to_vec();
        verified.sort();
        if expected.is_empty() || expected != verified {
            return Err(PlatformError::Conflict(
                "production verification must cover every exact release repository SHA",
            ));
        }
        for fact in &store.transition_verified_facts {
            let repository = fact["repository_id"]
                .as_str()
                .ok_or(PlatformError::Storage)?;
            let current =
                crate::task_transition_provider_authority::TaskTransitionProviderAuthority::lock(
                    &mut tx, ctx, project, repository,
                )?;
            if &current != fact {
                return Err(PlatformError::Conflict(
                    "repository authority changed before release publication",
                ));
            }
        }
        let source_count: i64 = tx
            .query_one(
                "SELECT count(*) FROM task_release_source_pins WHERE release_id=$1",
                &[&id],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        let source_rows=tx.query("SELECT b.head_sha,s.source_head_sha FROM task_release_source_pins s JOIN task_repository_branches b ON b.task_id=s.task_id AND b.repository_id=s.repository_id WHERE s.release_id=$1 ORDER BY s.task_id,s.repository_id FOR SHARE OF b",&[&id]).map_err(PlatformStore::database_error)?;
        if source_count == 0
            || source_rows.len() as i64 != source_count
            || source_rows
                .iter()
                .any(|row| row.get::<_, String>(0) != row.get::<_, String>(1))
        {
            return Err(PlatformError::Conflict(
                "task source branch changed after release selection",
            ));
        }
        let items=tx.query("WITH RECURSIVE ordered AS (SELECT i.task_id,i.task_revision,0 AS depth FROM task_release_items i WHERE i.release_id=$1 AND NOT EXISTS(SELECT 1 FROM task_subtasks s JOIN task_release_items p ON p.task_id=s.parent_task_id AND p.release_id=$1 WHERE s.child_task_id=i.task_id AND s.removed_at IS NULL) UNION ALL SELECT i.task_id,i.task_revision,p.depth+1 FROM task_release_items i JOIN task_subtasks s ON s.child_task_id=i.task_id AND s.removed_at IS NULL JOIN ordered p ON p.task_id=s.parent_task_id WHERE i.release_id=$1) SELECT task_id,task_revision FROM ordered ORDER BY depth DESC,task_id",&[&id]).map_err(PlatformStore::database_error)?;
        if items.is_empty() {
            return Err(PlatformError::Conflict("release has no selected tasks"));
        }
        for row in items {
            let task_id: String = row.get(0);
            let pinned: i64 = row.get(1);
            let before = TaskPolicyStore::scoped_task(&mut tx, Some(project), &task_id)?;
            if before.revision != pinned
                || before.archived
                || before.lifecycle != TaskLifecycle::Active
            {
                return Err(PlatformError::Conflict(
                    "release task changed before production publication",
                ));
            }
            crate::task_relation_policy::TaskRelationPolicy::state(&mut tx, &before, true)?;
            tx.execute("INSERT INTO task_production_publications(task_id,task_revision,release_id,evidence) VALUES($1,$2,$3,$4)",&[&task_id,&pinned,&id,evidence]).map_err(PlatformStore::database_error)?;
            let value:Value=tx.query_one("UPDATE tasks AS t SET lifecycle='completed',revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(t)",&[&task_id]).map_err(PlatformStore::database_error)?.get(0);
            let task: WorkTask = PlatformStore::decode(value)?;
            TaskHistoryWriter::event(
                &mut tx,
                ctx,
                &task,
                "task.production_published",
                json!({"before":TaskHistoryWriter::state(&before)?,"after":TaskHistoryWriter::state(&task)?,"release_id":id,"provider_verification":evidence}),
            )?;
        }
        tx.execute("UPDATE task_releases SET status='published',revision=revision+1,published_at=clock_timestamp(),publication_evidence=$2 WHERE id=$1",&[&id,&evidence]).map_err(PlatformStore::database_error)?;
        let value = Self::projection(&mut tx, project, id)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "release.production_published",
            Some(id),
            json!({"project_id":project,"provider_verification":evidence}),
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
}

#[cfg(test)]
#[path = "task_release_store_tests.rs"]
mod tests;
