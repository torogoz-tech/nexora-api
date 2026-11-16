use crate::{
    GitNetworkPolicy, PlatformStore, receipt_writer::ReceiptWriter,
    task_history_writer::TaskHistoryWriter, task_policy_store::TaskPolicyStore,
    task_scope_authority::TaskScopeAuthority, task_transition_provider::TaskTransitionProvider,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn task_commits(&mut self, ctx: &AuthContext, id: &str) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksRead)?;
        let items:Vec<Value>=tx.query("SELECT to_jsonb(l)-'provider_evidence' FROM task_commit_links l WHERE task_id=$1 AND removed_at IS NULL ORDER BY sequence,id",&[&id]).map_err(Self::database_error)?.into_iter().map(|r|r.get(0)).collect();
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":items}))
    }
    pub fn bind_task_commit(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &Value,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        let revision = input["expected_revision"]
            .as_i64()
            .ok_or(PlatformError::Validation("task revision required"))?;
        let repository = input["repository_id"]
            .as_str()
            .ok_or(PlatformError::Validation("repository required"))?;
        let commit = input["commit_sha"]
            .as_str()
            .ok_or(PlatformError::Validation("commit SHA required"))?;
        if input.as_object().is_none_or(|o| {
            o.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "expected_revision" | "repository_id" | "commit_sha"
                )
            })
        }) {
            return Err(PlatformError::Validation(
                "unsupported commit binding input",
            ));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let task = TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksWrite)?;
        TaskScopeAuthority::modify(&mut tx, ctx, task.project_id.as_deref(), id, false)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.commit_bind",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        if task.revision != revision || task.archived {
            return Err(PlatformError::Conflict(
                "task changed before commit binding",
            ));
        }
        let project = task
            .project_id
            .ok_or(PlatformError::Validation("task commits require a project"))?;
        if tx.query_one("SELECT EXISTS(SELECT 1 FROM task_commit_links WHERE task_id=$1 AND repository_id=$2 AND commit_sha=$3 AND removed_at IS NULL)",&[&id,&repository,&commit]).map_err(Self::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("commit is already linked to this task"));}
        let row=tx.query_opt("SELECT base_sha,head_sha FROM task_repository_branches WHERE task_id=$1 AND repository_id=$2",&[&id,&repository]).map_err(Self::database_error)?.ok_or(PlatformError::Conflict("verify the task repository branch before linking commits"))?;
        let base: String = row.get(0);
        let head: String = row.get(1);
        tx.commit().map_err(Self::database_error)?;
        self.transition_verified_facts.clear();
        let verification = TaskTransitionProvider::release_commit(
            self, ctx, &project, repository, &base, &head, commit, policy,
        );
        self.transition_verified_facts.clear();
        let evidence = verification?;
        self.write_verified_task_commit(
            ctx, &project, id, revision, repository, commit, &base, &head, &evidence,
        )
    }
}
impl PlatformStore {
    pub fn remove_task_commit(
        &mut self,
        ctx: &AuthContext,
        task: &str,
        link: &str,
        revision: i64,
    ) -> Result<Value, PlatformError> {
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let current = TaskScopeAuthority::task(&mut tx, ctx, task, Capability::TasksWrite)?;
        TaskScopeAuthority::modify(&mut tx, ctx, current.project_id.as_deref(), task, false)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.commit_remove",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        let before = TaskPolicyStore::scoped_task(&mut tx, current.project_id.as_deref(), task)?;
        if before.revision != revision || before.archived {
            return Err(PlatformError::Conflict(
                "task changed before commit reference removal",
            ));
        }
        let value:Value=tx.query_opt("UPDATE task_commit_links AS l SET removed_at=clock_timestamp() WHERE id=$1 AND task_id=$2 AND removed_at IS NULL RETURNING to_jsonb(l)-'provider_evidence'",&[&link,&task]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        let after:WorkTask=Self::decode(tx.query_one("UPDATE tasks AS t SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(t)",&[&task]).map_err(Self::database_error)?.get(0))?;
        TaskHistoryWriter::event(
            &mut tx,
            ctx,
            &after,
            "task.commit_removed",
            json!({"before":TaskHistoryWriter::state(&before)?,"after":TaskHistoryWriter::state(&after)?,"commit":value}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
