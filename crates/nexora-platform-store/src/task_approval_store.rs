use crate::{
    PlatformStore, SecretGenerator, receipt_writer::ReceiptWriter,
    task_approval_fingerprint::TaskApprovalFingerprint, task_history_writer::TaskHistoryWriter,
    task_policy_store::TaskPolicyStore, task_transition_evaluation::TaskTransitionEvaluation,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};

pub(crate) struct TaskApprovalStore;
impl TaskApprovalStore {
    pub fn grant(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
        input: &TaskApprovalGrant,
    ) -> Result<TaskApproval, PlatformError> {
        if ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        if !(1..=86400).contains(&input.expires_in_seconds)
            || input.parameters.approval_id.is_some()
        {
            return Err(PlatformError::Validation(
                "invalid approval duration or nested approval",
            ));
        }
        DomainText::required(&input.reason, 4000)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_scope_authority::TaskScopeAuthority::require(
            &mut tx,
            ctx,
            Capability::ApprovalsGrant,
            project,
        )?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.approval_grant",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let result = Self::grant_in(&mut tx, ctx, project, id, input)?;
        // A direct session grant also resolves matching inbox requests, keeping one approval operation visible.
        let pending=tx.query("UPDATE approval_requests SET status='approved',revision=revision+1,task_approval_id=$2,decided_by=$3,decided_credential_id=$4,decision_reason=$5,decided_at=clock_timestamp() WHERE target_kind='task' AND resource_id=$1 AND status='pending' AND expires_at>clock_timestamp() AND operation_checksum=$6 RETURNING id",&[&id,&result.id,&ctx.actor_id(),&ctx.credential_id(),&result.reason,&result.parameters_checksum]).map_err(PlatformStore::database_error)?;
        for row in pending {
            let request = crate::approval_request_query_store::ApprovalRequestQueryStore::row(
                &mut tx,
                project.ok_or(PlatformError::Storage)?,
                &row.get::<_, String>(0),
                false,
            )?;
            crate::approval_request_history_writer::ApprovalRequestHistoryWriter::append(
                &mut tx,
                ctx,
                &request,
                "approval_request.decided",
                &result.reason,
            )?;
        }
        let value = serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }

    pub(crate) fn grant_in(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
        input: &TaskApprovalGrant,
    ) -> Result<TaskApproval, PlatformError> {
        if ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        if !(1..=86400).contains(&input.expires_in_seconds)
            || input.parameters.approval_id.is_some()
        {
            return Err(PlatformError::Validation(
                "invalid approval duration or nested approval",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        crate::task_scope_authority::TaskScopeAuthority::require(
            tx,
            ctx,
            Capability::ApprovalsGrant,
            project,
        )?;
        let task = TaskPolicyStore::scoped_task(tx, project, id)?;
        if task.archived || task.revision != input.parameters.expected_revision {
            return Err(PlatformError::Conflict("task revision changed or archived"));
        }
        let flow = crate::task_pool_workflow::TaskPoolWorkflow::flow(
            tx,
            project,
            task.execution_version()?,
            false,
        )?;
        let mut evaluation = TaskTransitionEvaluation::build(
            tx,
            &task,
            &input.parameters,
            &flow,
            &input.transition_id,
        )?;
        evaluation.human_approval_valid = true;
        let transition =
            flow.graph
                .transition(task.execution_state()?, &input.transition_id, &evaluation)?;
        if !transition.requires_approval() {
            return Err(PlatformError::Validation(
                "transition does not require human approval",
            ));
        }
        if transition.requires_approval() {
            let own:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_transition_runs WHERE task_id=$1 AND task_revision=$2 AND transition_id=$3 AND actor_id=$4 AND record->>'status' IN ('waiting_approval','ready','running'))",&[&id,&task.revision,&transition.id,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.get(0);
            if own {
                return Err(PlatformError::Forbidden);
            }
        }
        let parameters = TaskApprovalFingerprint::parameters(&input.parameters)?;
        let checksum = TaskApprovalFingerprint::checksum(&task, transition, &input.parameters)?;
        let approval = SecretGenerator::id("tap")?;
        let seconds = i64::from(input.expires_in_seconds);
        let value:Value=tx.query_one("INSERT INTO task_approvals AS a(id,project_id,task_id,task_revision,workflow_id,workflow_version_id,transition_id,from_state,to_state,parameters,parameters_checksum,reason,granted_by,credential_id,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,clock_timestamp()+$15::bigint*interval '1 second') RETURNING to_jsonb(a)",&[&approval,&project,&id,&task.revision,&task.workflow_id,&task.workflow_version_id,&transition.id,&transition.from,&transition.to,&parameters,&checksum,&reason,&ctx.actor_id(),&ctx.credential_id(),&seconds]).map_err(PlatformStore::database_error)?.get(0);
        let fingerprint = SecretGenerator::checksum(
            &serde_json::to_vec(&parameters).map_err(|_| PlatformError::Storage)?,
        );
        tx.execute("UPDATE task_transition_runs SET parameters=parameters||jsonb_build_object('approval_id',$5::text),record=record||jsonb_build_object('status','ready','error_code',NULL,'message',NULL),updated_at=clock_timestamp() WHERE task_id=$1 AND task_revision=$2 AND transition_id=$3 AND fingerprint=$4 AND record->>'status'='waiting_approval'", &[&id,&task.revision,&transition.id,&fingerprint,&approval]).map_err(PlatformStore::database_error)?;
        TaskHistoryWriter::event(
            tx,
            ctx,
            &task,
            "task.approval_granted",
            json!({"approval_id":approval,"transition_id":transition.id,"parameters_checksum":checksum,"reason":reason}),
        )?;
        PlatformStore::decode(value)
    }

    pub fn consume(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        task: &WorkTask,
        flow: &WorkflowVersion,
        transition_id: &str,
        input: &TaskTransitionRequest,
    ) -> Result<bool, PlatformError> {
        let Some(id) = &input.approval_id else {
            return Ok(false);
        };
        let transition = flow
            .graph
            .transitions
            .iter()
            .find(|edge| {
                edge.id == transition_id && task.state_id.as_deref() == Some(edge.from.as_str())
            })
            .ok_or(PlatformError::Validation("transition is not available"))?;
        if !transition.requires_approval() {
            return Err(PlatformError::Validation(
                "transition does not accept an approval",
            ));
        }
        let value:Value=tx.query_opt("SELECT to_jsonb(a) FROM task_approvals a WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2 AND task_id=$3 AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at>clock_timestamp() FOR UPDATE",&[&id,&task.project_id,&task.id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("approval is unavailable"))?.get(0);
        let approval: TaskApproval = PlatformStore::decode(value)?;
        if transition.requires_approval() && approval.granted_by == ctx.actor_id() {
            return Err(PlatformError::Forbidden);
        }
        if approval.task_revision != task.revision
            || approval.workflow_version_id != flow.id
            || approval.workflow_id != flow.workflow_id
            || approval.transition_id != transition.id
            || task.state_id.as_deref() != Some(approval.from_state.as_str())
            || approval.to_state != transition.to
            || approval.parameters_checksum
                != TaskApprovalFingerprint::checksum(task, transition, input)?
        {
            return Err(PlatformError::Conflict(
                "approval does not match this operation",
            ));
        }
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND active AND kind='human' FOR SHARE",
            &[&approval.granted_by],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::Conflict("approval grantor is inactive"))?;
        let updated=tx.execute("UPDATE task_approvals SET consumed_at=clock_timestamp(),consumed_by=$2,consumed_credential_id=$3 WHERE id=$1 AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at>clock_timestamp()",&[&id,&ctx.actor_id(),&ctx.credential_id()]).map_err(PlatformStore::database_error)?;
        if updated != 1 {
            return Err(PlatformError::Conflict("approval expired"));
        }
        TaskHistoryWriter::event(
            tx,
            ctx,
            task,
            "task.approval_consumed",
            json!({"approval_id":id,"transition_id":transition.id,"parameters_checksum":approval.parameters_checksum}),
        )?;
        Ok(true)
    }
}
