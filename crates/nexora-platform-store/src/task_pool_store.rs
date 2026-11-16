use crate::{
    PlatformStore, SecretGenerator, receipt_writer::ReceiptWriter,
    task_history_writer::TaskHistoryWriter, task_policy_store::TaskPolicyStore,
    task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use serde_json::Value;

pub(crate) struct TaskPoolStore;
impl TaskPoolStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        input: &TaskCreate,
    ) -> Result<WorkTask, PlatformError> {
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_relation_lock::TaskRelationLock::acquire(&mut tx)?;
        TaskScopeAuthority::require(&mut tx, ctx, Capability::TasksWrite, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let task = Self::insert(&mut tx, ctx, project, input, None)?;
        ReceiptWriter::save(
            &mut tx,
            request.as_ref(),
            &serde_json::to_value(&task).map_err(|_| PlatformError::Storage)?,
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(task)
    }
    pub fn insert(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        project: Option<&str>,
        input: &TaskCreate,
        template: Option<&TaskTemplateVersion>,
    ) -> Result<WorkTask, PlatformError> {
        let title = DomainText::required(&input.title, 256)?;
        let context = TaskPolicyStore::context(&input.context)?;
        let criteria =
            crate::task_acceptance_criteria::task_acceptance_criteria(&input.acceptance_criteria)?;
        let origin = DomainText::required(&input.origin, 128)?;
        let reference = input
            .origin_reference
            .as_deref()
            .map(|v| DomainText::required(v, 2048))
            .transpose()?;
        let fields = TaskPolicyStore::normalize_fields(tx, &input.fields)?;
        let code = input.type_code.trim().to_ascii_lowercase();
        let purpose = crate::task_purpose_policy::TaskPurposePolicy::validate(
            tx,
            project,
            &code,
            input.primary_requirement_id.as_deref(),
            input.primary_feature_id.as_deref(),
            input.entry_workflow_id.as_deref(),
            input.assignee_id.as_deref(),
        )?;
        if let Some(project) = project {
            TaskPolicyStore::kind(tx, project, &code)?;
        } else {
            tx.query_opt(
                "SELECT code FROM task_type_catalog WHERE code=$1 AND NOT archived FOR SHARE",
                &[&code],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Validation(
                "catalog task type is unknown or archived",
            ))?;
        }
        if let Some(actor) = &input.assignee_id {
            TaskScopeAuthority::assignee(tx, project, actor)?;
        }
        let position = if let Some(backlog) = &input.backlog_id {
            let project = project.ok_or(PlatformError::Validation(
                "projectless tasks cannot belong to a backlog",
            ))?;
            if TaskPolicyStore::backlog(tx, project, backlog)?.archived {
                return Err(PlatformError::Conflict("backlog is archived"));
            }
            let previous: Option<i64> = tx
                .query_one(
                    "SELECT max(position) FROM tasks WHERE backlog_id=$1",
                    &[&backlog],
                )
                .map_err(PlatformStore::database_error)?
                .get(0);
            Some(
                previous
                    .unwrap_or(0)
                    .checked_add(1024)
                    .ok_or(PlatformError::Conflict("backlog ordering exhausted"))?,
            )
        } else {
            None
        };
        let template_version = template.map(|v| v.id.as_str());
        let template_snapshot = template
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| PlatformError::Storage)?;
        let id = SecretGenerator::id("tsk")?;
        let value:Value=tx.query_one("INSERT INTO tasks AS t(id,project_id,title,context,type_code,priority,assignee_id,backlog_id,origin,origin_reference,fields,position,created_by,template_version_id,template_snapshot,acceptance_criteria,purpose,entry_workflow_id,primary_requirement_id,primary_feature_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20) RETURNING to_jsonb(t)",&[&id,&project,&title,&context,&code,&input.priority.as_str(),&input.assignee_id,&input.backlog_id,&origin,&reference,&fields,&position,&ctx.actor_id(),&template_version,&template_snapshot,&criteria,&purpose.as_str(),&input.entry_workflow_id,&input.primary_requirement_id,&input.primary_feature_id]).map_err(PlatformStore::database_error)?.get(0);
        let task: WorkTask = PlatformStore::decode(value)?;
        if let Some(backlog) = &input.backlog_id {
            tx.execute(
                "UPDATE backlogs SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1",
                &[&backlog],
            )
            .map_err(PlatformStore::database_error)?;
        }
        crate::task_evidence_binding::TaskEvidenceBinding::claim(tx, ctx, &task.id, &task.context)?;
        TaskHistoryWriter::append(tx, ctx, &task, "task.created", None, None)?;
        Ok(task)
    }
    pub fn take(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        id: &str,
        input: &TaskTake,
        expected_project: Option<&str>,
    ) -> Result<WorkTask, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation("invalid task revision"));
        }
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_relation_lock::TaskRelationLock::acquire(&mut tx)?;
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND active AND kind='human' FOR UPDATE",
            &[&ctx.actor_id()],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::Unauthorized)?;
        let before = TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksTransition)?;
        let before = TaskPolicyStore::scoped_task(&mut tx, before.project_id.as_deref(), id)?;
        if expected_project.is_some() && before.project_id.as_deref() != expected_project {
            return Err(PlatformError::NotFound);
        }
        TaskScopeAuthority::assignee(&mut tx, before.project_id.as_deref(), ctx.actor_id())?;
        tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('developer','technical_lead','pmo','manager','super_admin') FOR SHARE",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Forbidden)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.take",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        if before.revision != input.expected_revision {
            return Err(PlatformError::Conflict("task revision changed"));
        }
        if !((before.lifecycle == TaskLifecycle::Pending)
            || (before.lifecycle == TaskLifecycle::Active
                && before.assignee_id.is_none()
                && before.workflow_id.is_some()))
            || before.archived
        {
            return Err(PlatformError::Conflict("only pending tasks can be taken"));
        }
        if before
            .assignee_id
            .as_deref()
            .is_some_and(|actor| actor != ctx.actor_id())
        {
            return Err(PlatformError::Conflict(
                "task is reserved for another developer",
            ));
        }
        if before.governed_stage
            && before.purpose == TaskPurpose::ProductDesign
            && before.primary_requirement_id.is_none()
        {
            return Err(PlatformError::Validation(
                "product design requires a primary requirement before taking the task",
            ));
        }
        crate::task_relation_policy::TaskRelationPolicy::take(&mut tx, &before)?;
        if before.governed_stage
            && input
                .workflow_id
                .as_ref()
                .is_some_and(|id| before.entry_workflow_id.as_ref() != Some(id))
        {
            return Err(PlatformError::Validation(
                "select the entry workflow on the task before taking it",
            ));
        }
        let legacy_entry = if !before.governed_stage && before.entry_workflow_id.is_none() {
            crate::developer_task_policy::DeveloperTaskPolicy::take_version(
                &mut tx,
                ctx.actor_id(),
                input.workflow_id.as_deref(),
            )?
            .map(|version| {
                crate::task_pool_workflow::TaskPoolWorkflow::flow(
                    &mut tx,
                    before.project_id.as_deref(),
                    &version,
                    true,
                )
                .map(|flow| flow.workflow_id)
            })
            .transpose()?
        } else {
            None
        };
        let (workflow_id, version_id, state_id) = if let Some(entry) =
            before.entry_workflow_id.as_ref().or(legacy_entry.as_ref())
        {
            let assigned_version =
                crate::developer_task_policy::DeveloperTaskPolicy::assigned_version(
                    &mut tx,
                    ctx.actor_id(),
                    entry,
                )?
                .ok_or(PlatformError::Validation(
                    "entry workflow must be assigned to the member",
                ))?;
            // A handoff pins the task's destination revision; membership establishes eligibility only.
            let handoff = before.lifecycle == TaskLifecycle::Active && before.assignee_id.is_none();
            let parent_version:Option<String>=tx.query_opt("SELECT p.workflow_version_id FROM task_subtasks s JOIN tasks p ON p.id=s.parent_task_id WHERE s.child_task_id=$1 AND s.removed_at IS NULL AND s.work_kind='decomposition' FOR SHARE OF p",&[&id]).map_err(PlatformStore::database_error)?.and_then(|row|row.get(0));
            let version = if handoff {
                before
                    .workflow_version_id
                    .as_ref()
                    .ok_or(PlatformError::Conflict("handoff workflow version missing"))?
            } else {
                parent_version.as_ref().unwrap_or(&assigned_version)
            };
            let flow = crate::task_pool_workflow::TaskPoolWorkflow::flow(
                &mut tx,
                before.project_id.as_deref(),
                version,
                true,
            )?;
            let state = if handoff {
                flow.graph
                    .states
                    .iter()
                    .find(|state| Some(state.id.as_str()) == before.state_id.as_deref())
            } else {
                flow.graph.states.iter().find(|state| state.initial)
            }
            .ok_or(PlatformError::Validation("workflow initial state missing"))?;
            crate::task_relation_policy::TaskRelationPolicy::state(
                &mut tx,
                &before,
                state.terminal,
            )?;
            (
                Some(flow.workflow_id),
                Some(flow.id),
                Some(state.id.clone()),
            )
        } else if before.purpose == TaskPurpose::General {
            (None, None, None)
        } else {
            return Err(PlatformError::Validation(
                "select the entry workflow on the task before taking it",
            ));
        };
        crate::task_subtask_stage_policy::TaskSubtaskStagePolicy::validate(
            &mut tx,
            id,
            workflow_id.as_deref(),
            version_id.as_deref(),
        )?;
        let value:Value=tx.query_one("UPDATE tasks AS t SET assignee_id=$2,workflow_id=$3,workflow_version_id=$4,state_id=$5,lifecycle='active',revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(t)",&[&id,&ctx.actor_id(),&workflow_id,&version_id,&state_id]).map_err(PlatformStore::database_error)?.get(0);
        let task: WorkTask = PlatformStore::decode(value)?;
        TaskHistoryWriter::append(&mut tx, ctx, &task, "task.taken", Some(&before), None)?;
        ReceiptWriter::save(
            &mut tx,
            request.as_ref(),
            &serde_json::to_value(&task).map_err(|_| PlatformError::Storage)?,
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(task)
    }
}
