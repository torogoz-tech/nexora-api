use crate::{
    PlatformStore, SecretGenerator, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter, task_history_writer::TaskHistoryWriter,
    task_policy_store::TaskPolicyStore,
};
use nexora_platform::*;
use serde_json::Value;

pub(crate) struct TaskDefinitionStore;
impl TaskDefinitionStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &TaskCreate,
        require_developer: bool,
    ) -> Result<WorkTask, PlatformError> {
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_relation_lock::TaskRelationLock::acquire(&mut tx)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::TasksWrite, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
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
        let task = Self::insert(&mut tx, ctx, project, input, None, require_developer)?;
        let value = serde_json::to_value(&task).map_err(|_| PlatformError::Storage)?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(task)
    }
    pub fn insert(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        input: &TaskCreate,
        template: Option<&TaskTemplateVersion>,
        require_developer: bool,
    ) -> Result<WorkTask, PlatformError> {
        if require_developer {
            return crate::task_pool_store::TaskPoolStore::insert(
                tx,
                ctx,
                Some(project),
                input,
                template,
            );
        }
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
        let template_version = template.map(|version| version.id.as_str());
        let template_snapshot = template
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| PlatformError::Storage)?;
        let binding = input
            .assignee_id
            .as_deref()
            .map(|actor| {
                crate::developer_task_policy::DeveloperTaskPolicy::version(tx, actor, true)
            })
            .transpose()?
            .flatten();
        if require_developer && binding.is_none() {
            return Err(PlatformError::Validation(
                "assign a developer with a published workflow before creating tasks",
            ));
        }
        let backlog = input
            .backlog_id
            .as_deref()
            .map(|id| TaskPolicyStore::backlog(tx, project, id))
            .transpose()?;
        if backlog.as_ref().is_some_and(|backlog| backlog.archived) {
            return Err(PlatformError::Conflict("backlog is archived"));
        }
        let version = if let Some(binding) = binding {
            if input
                .workflow_version_id
                .as_ref()
                .is_some_and(|id| id != &binding)
            {
                return Err(PlatformError::Validation(
                    "new tasks use the assigned developer workflow version",
                ));
            }
            binding
        } else if let Some(backlog) = backlog {
            if input
                .workflow_version_id
                .as_ref()
                .is_some_and(|id| id != &backlog.workflow_version_id)
            {
                return Err(PlatformError::Conflict(
                    "task workflow differs from backlog default",
                ));
            }
            backlog.workflow_version_id
        } else {
            input
                .workflow_version_id
                .clone()
                .ok_or(PlatformError::Validation(
                    "assign a developer with a published workflow",
                ))?
        };
        let flow = TaskPolicyStore::flow(tx, project, &version, true)?;
        if flow.project_id.is_none()
            && input
                .assignee_id
                .as_deref()
                .map(|actor| {
                    crate::developer_task_policy::DeveloperTaskPolicy::version(tx, actor, true)
                })
                .transpose()?
                .flatten()
                .is_none()
        {
            return Err(PlatformError::Validation(
                "assign a developer with a published workflow",
            ));
        }
        let initial = flow
            .graph
            .states
            .iter()
            .find(|state| state.initial)
            .ok_or(PlatformError::Validation("workflow initial state missing"))?;
        TaskPolicyStore::kind(tx, project, &code)?;
        if let Some(actor) = &input.assignee_id {
            TaskPolicyStore::actor(tx, actor)?;
        }
        let position = if let Some(backlog) = &input.backlog_id {
            let previous: Option<i64> = tx
                .query_one(
                    "SELECT max(position) FROM tasks WHERE project_id=$1 AND backlog_id=$2",
                    &[&project, &backlog],
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
        let id = SecretGenerator::id("tsk")?;
        let value:Value=tx.query_one("INSERT INTO tasks AS t(id,project_id,title,context,type_code,priority,assignee_id,backlog_id,workflow_id,workflow_version_id,state_id,origin,origin_reference,fields,position,created_by,template_version_id,template_snapshot,lifecycle,acceptance_criteria) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,'active',$19) RETURNING to_jsonb(t)",&[&id,&project,&title,&context,&code,&input.priority.as_str(),&input.assignee_id,&input.backlog_id,&flow.workflow_id,&flow.id,&initial.id,&origin,&reference,&fields,&position,&ctx.actor_id(),&template_version,&template_snapshot,&criteria]).map_err(PlatformStore::database_error)?.get(0);
        let task: WorkTask = PlatformStore::decode(value.clone())?;
        if let Some(backlog) = &task.backlog_id {
            tx.execute("UPDATE backlogs SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2",&[&backlog,&project]).map_err(PlatformStore::database_error)?;
        }
        crate::task_evidence_binding::TaskEvidenceBinding::claim(tx, ctx, &task.id, &task.context)?;
        TaskHistoryWriter::append(tx, ctx, &task, "task.created", None, None)?;
        Ok(task)
    }
    pub fn change(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
        input: &TaskChange,
    ) -> Result<WorkTask, PlatformError> {
        if input.expected_revision < 1 || (input.assignee_id.is_some() && input.clear_assignee) {
            return Err(PlatformError::Validation(
                "invalid task revision or assignee change",
            ));
        }
        if input.acceptance_criteria.is_none()
            && input.title.is_none()
            && input.context.is_none()
            && input.type_code.is_none()
            && input.entry_workflow_id.is_none()
            && !input.clear_entry_workflow
            && input.primary_requirement_id.is_none()
            && input.primary_feature_id.is_none()
            && !input.clear_primary_knowledge
            && input.priority.is_none()
            && input.assignee_id.is_none()
            && !input.clear_assignee
            && input.block.is_none()
            && input.fields.is_none()
            && input.archived.is_none()
        {
            return Err(PlatformError::Validation("task change has no fields"));
        }
        let criteria = input
            .acceptance_criteria
            .as_ref()
            .map(|items| crate::task_acceptance_criteria::task_acceptance_criteria(items))
            .transpose()?;
        let reason = DomainText::required(&input.reason, 4000)?;
        let title = input
            .title
            .as_deref()
            .map(|v| DomainText::required(v, 256))
            .transpose()?;
        let context = input
            .context
            .as_deref()
            .map(TaskPolicyStore::context)
            .transpose()?;
        if let Some(fields) = &input.fields {
            TaskPolicyStore::fields(fields)?;
        }
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_relation_lock::TaskRelationLock::acquire(&mut tx)?;
        crate::task_scope_authority::TaskScopeAuthority::require(
            &mut tx,
            ctx,
            Capability::TasksWrite,
            project,
        )?;
        if let Some(actor) = &input.assignee_id {
            crate::task_scope_authority::TaskScopeAuthority::assignee(&mut tx, project, actor)?;
        }
        crate::task_scope_authority::TaskScopeAuthority::modify(
            &mut tx,
            ctx,
            project,
            id,
            input.assignee_id.as_deref() == Some(ctx.actor_id())
                && input.acceptance_criteria.is_none()
                && input.title.is_none()
                && input.context.is_none()
                && input.type_code.is_none()
                && input.entry_workflow_id.is_none()
                && !input.clear_entry_workflow
                && input.primary_requirement_id.is_none()
                && input.primary_feature_id.is_none()
                && !input.clear_primary_knowledge
                && input.priority.is_none()
                && input.block.is_none()
                && input.fields.is_none()
                && input.archived.is_none(),
        )?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.change",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let mut before = TaskPolicyStore::scoped_task(&mut tx, project, id)?;
        if before.revision != input.expected_revision {
            return Err(PlatformError::Conflict("task revision changed"));
        }
        if before.archived && input.archived != Some(false) {
            return Err(PlatformError::Conflict("task is archived"));
        }
        if input.archived.is_some() {
            let role: String = tx.query_opt("SELECT role FROM actor_roles WHERE actor_id=$1 AND role IN ('technical_lead','pmo','manager','super_admin') FOR SHARE", &[&ctx.actor_id()])
                .map_err(PlatformStore::database_error)?.ok_or(PlatformError::Forbidden)?.get(0);
            let manager = matches!(role.as_str(), "manager" | "super_admin");
            if input.archived == Some(true)
                && !manager
                && matches!(
                    before.lifecycle,
                    TaskLifecycle::Active | TaskLifecycle::Paused
                )
            {
                return Err(PlatformError::Conflict(
                    "active or paused tasks cannot be archived; return the task to pending or complete it first",
                ));
            }
            if input.archived == Some(true) && before.lifecycle == TaskLifecycle::Active {
                let value: Value = tx.query_one("UPDATE tasks AS t SET lifecycle='paused',revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(t)", &[&id]).map_err(PlatformStore::database_error)?.get(0);
                let paused: WorkTask = PlatformStore::decode(value)?;
                TaskHistoryWriter::append(
                    &mut tx,
                    ctx,
                    &paused,
                    "task.pause",
                    Some(&before),
                    Some(&reason),
                )?;
                before = paused;
            }
        }
        let mut current = before.clone();
        if let Some(criteria) = criteria {
            current.acceptance_criteria = PlatformStore::decode(criteria)?;
        }
        if let Some(title) = title {
            current.title = title;
        }
        if let Some(context) = context {
            current.context = context;
        }
        if let Some(code) = &input.type_code {
            let code = code.trim().to_ascii_lowercase();
            if let Some(project) = project {
                TaskPolicyStore::kind(&mut tx, project, &code)?;
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
            current.type_code = code;
        }
        if input.entry_workflow_id.is_some() || input.clear_entry_workflow {
            if before.lifecycle != TaskLifecycle::Pending {
                return Err(PlatformError::Conflict(
                    "entry workflow can only change on pending tasks",
                ));
            }
            if input.clear_entry_workflow && input.entry_workflow_id.is_some() {
                return Err(PlatformError::Validation(
                    "conflicting entry workflow change",
                ));
            }
            current.entry_workflow_id = input.entry_workflow_id.clone();
        }
        if input.clear_primary_knowledge {
            current.primary_requirement_id = None;
            current.primary_feature_id = None;
        }
        if input.primary_requirement_id.is_some() {
            current.primary_requirement_id = input.primary_requirement_id.clone();
            current.primary_feature_id = None;
        }
        if input.primary_feature_id.is_some() {
            current.primary_feature_id = input.primary_feature_id.clone();
            current.primary_requirement_id = None;
        }
        if input.primary_requirement_id.is_some() && input.primary_feature_id.is_some() {
            return Err(PlatformError::Validation(
                "select at most one primary requirement or feature",
            ));
        }
        if let Some(priority) = input.priority {
            current.priority = priority;
        }
        if let Some(actor) = &input.assignee_id {
            TaskPolicyStore::actor(&mut tx, actor)?;
            current.assignee_id = Some(actor.clone());
        } else if input.clear_assignee {
            current.assignee_id = None;
        }
        if let Some(block) = &input.block {
            current.blocked = block.blocked;
            if block.blocked {
                current.block_reason = Some(DomainText::required(
                    block
                        .reason
                        .as_deref()
                        .ok_or(PlatformError::Validation("block reason required"))?,
                    2000,
                )?);
                let actor = block
                    .blocker_id
                    .as_deref()
                    .ok_or(PlatformError::Validation(
                        "block responsible actor required",
                    ))?;
                TaskPolicyStore::actor(&mut tx, actor)?;
                current.blocker_id = Some(actor.into());
            } else {
                current.block_reason = None;
                current.blocker_id = None;
            }
        }
        if let Some(fields) = &input.fields {
            current.fields = fields.clone();
        }
        if let Some(archived) = input.archived {
            current.archived = archived;
        }
        if input.assignment_state_id.is_some() || input.assignment_workflow_version_id.is_some() {
            return Err(PlatformError::Validation(
                "assignment does not choose a workflow or state",
            ));
        }
        if before.assignee_id != current.assignee_id
            && matches!(
                before.lifecycle,
                TaskLifecycle::Active | TaskLifecycle::Paused
            )
        {
            current.lifecycle = TaskLifecycle::Pending;
            current.workflow_id = None;
            current.workflow_version_id = None;
            current.state_id = None;
        }
        if input.type_code.is_some()
            || input.entry_workflow_id.is_some()
            || input.clear_entry_workflow
            || input.primary_requirement_id.is_some()
            || input.primary_feature_id.is_some()
            || input.clear_primary_knowledge
            || input.assignee_id.is_some()
        {
            let purpose = crate::task_purpose_policy::TaskPurposePolicy::validate(
                &mut tx,
                project,
                &current.type_code,
                current.primary_requirement_id.as_deref(),
                current.primary_feature_id.as_deref(),
                current.entry_workflow_id.as_deref(),
                current.assignee_id.as_deref(),
            )?;
            if before.lifecycle != TaskLifecycle::Pending && purpose != before.purpose {
                return Err(PlatformError::Conflict(
                    "task purpose cannot change during execution",
                ));
            }
            current.purpose = purpose;
        }
        let fields = TaskPolicyStore::normalize_fields(&mut tx, &current.fields)?;
        if current.workflow_id != before.workflow_id
            || current.entry_workflow_id != before.entry_workflow_id
            || current.workflow_version_id != before.workflow_version_id
        {
            crate::task_subtask_stage_policy::TaskSubtaskStagePolicy::validate(
                &mut tx,
                id,
                current
                    .workflow_id
                    .as_deref()
                    .or(current.entry_workflow_id.as_deref()),
                current.workflow_version_id.as_deref(),
            )?;
        }
        let value:Value=tx.query_one("UPDATE tasks AS t SET title=$3,context=$4,type_code=$5,priority=$6,assignee_id=$7,blocked=$8,block_reason=$9,blocker_id=$10,fields=$11,archived=$12,workflow_id=$13,workflow_version_id=$14,state_id=$15,lifecycle=$16,acceptance_criteria=$17,purpose=$18,entry_workflow_id=$19,primary_requirement_id=$20,primary_feature_id=$21,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2 RETURNING to_jsonb(t)",&[&id,&project,&current.title,&current.context,&current.type_code,&current.priority.as_str(),&current.assignee_id,&current.blocked,&current.block_reason,&current.blocker_id,&fields,&current.archived,&current.workflow_id,&current.workflow_version_id,&current.state_id,&current.lifecycle.as_str(),&serde_json::json!(current.acceptance_criteria),&current.purpose.as_str(),&current.entry_workflow_id,&current.primary_requirement_id,&current.primary_feature_id]).map_err(PlatformStore::database_error)?.get(0);
        let result: WorkTask = PlatformStore::decode(value.clone())?;
        crate::task_evidence_binding::TaskEvidenceBinding::claim(
            &mut tx,
            ctx,
            &result.id,
            &result.context,
        )?;
        TaskHistoryWriter::append(
            &mut tx,
            ctx,
            &result,
            if before.workflow_id != current.workflow_id {
                "task.reassigned_workflow"
            } else {
                "task.changed"
            },
            Some(&before),
            Some(&reason),
        )?;
        let public = serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &public)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
