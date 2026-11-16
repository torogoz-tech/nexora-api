use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter, task_policy_store::TaskPolicyStore,
    task_work_store::TaskWorkStore,
};
use nexora_platform::*;
use serde_json::json;
/// Persists declarations received through MCP. Does not run Git or inspect client files.
pub(crate) struct TaskWorkReportStore;
impl TaskWorkReportStore {
    pub fn report(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        task: &str,
        id: &str,
        input: &TaskWorkReport,
    ) -> Result<TaskWorkSession, PlatformError> {
        if input.branches.is_empty() || input.branches.len() > 8 {
            return Err(PlatformError::Validation("report requires 1..8 branches"));
        }
        let mut branches = input.branches.clone();
        branches.sort_by(|a, b| a.repository_id.cmp(&b.repository_id));
        for (index, branch) in branches.iter().enumerate() {
            if !TaskWorkBranchReport::valid_sha(&branch.base_sha)
                || !TaskWorkBranchReport::valid_sha(&branch.head_sha)
                || branch.base_sha.len() != branch.head_sha.len()
                || !DevelopmentPolicy::valid_ref(&branch.branch_name)
                || branch.repository_id.is_empty()
                || branch.repository_id.len() > 128
                || index > 0 && branches[index - 1].repository_id == branch.repository_id
            {
                return Err(PlatformError::Validation(
                    "invalid or duplicate branch report",
                ));
            }
        }
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        for cap in [
            Capability::TasksRead,
            Capability::TasksWrite,
            Capability::TasksTransition,
        ] {
            PlatformAuthority::require(&mut tx, ctx, cap, Some(project))?;
        }
        PlatformAuthority::project(&mut tx, project)?;
        let current = TaskPolicyStore::task(&mut tx, project, task)?;
        let mut session = TaskWorkStore::session(&mut tx, ctx, project, task, id)?;
        TaskWorkStore::policies(&mut tx, ctx, &session)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.work.report",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        if current.governed_stage
            && branches
                .iter()
                .zip(&session.branches)
                .any(|(reported, expected)| expected.base_sha.as_ref() != Some(&reported.base_sha))
        {
            return Err(PlatformError::Conflict(
                "branch report must preserve the provider-created branch base SHA",
            ));
        }
        if branches.len() != session.branches.len()
            || branches
                .iter()
                .zip(&session.branches)
                .any(|(reported, expected)| {
                    reported.repository_id != expected.repository_id
                        || reported.branch_name != expected.branch_name
                })
        {
            return Err(PlatformError::Validation(
                "report does not match prepared repositories and branch names",
            ));
        }
        if session.report.is_some() {
            if branches.iter().zip(&session.branches).any(|(a, b)| {
                Some(&a.base_sha) != b.base_sha.as_ref() || Some(&a.head_sha) != b.head_sha.as_ref()
            }) {
                return Err(PlatformError::Conflict(
                    "branch report is immutable; create a new work session",
                ));
            }
        } else {
            if current.archived
                || current.revision != session.task_revision
                || current.workflow_version_id.as_deref()
                    != Some(session.workflow_version_id.as_str())
            {
                return Err(PlatformError::Conflict("task changed before branch report"));
            }
            for (branch, reported) in session.branches.iter_mut().zip(branches) {
                branch.base_sha = Some(reported.base_sha);
                branch.head_sha = Some(reported.head_sha);
            }
            let reported_at: String = tx
                .query_one("SELECT clock_timestamp()::text", &[])
                .map_err(PlatformStore::database_error)?
                .get(0);
            session.report = Some(TaskWorkReportReceipt {
                provenance: TaskWorkProvenance::AgentReported,
                credential_id: ctx.credential_id().into(),
                reported_at,
            });
            let value = serde_json::to_value(&session).map_err(|_| PlatformError::Storage)?;
            tx.execute(
                "UPDATE task_work_sessions SET reported=true,record=$2 WHERE id=$1",
                &[&id, &value],
            )
            .map_err(PlatformStore::database_error)?;
            AuditWriter::append(
                &mut tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                "task.work.reported",
                Some(task),
                json!({"session_id":id,"provenance":"agent_reported"}),
            )?;
        }
        let value = serde_json::to_value(&session).map_err(|_| PlatformError::Storage)?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(session)
    }
}
impl PlatformStore {
    pub fn report_task_work(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        task: &str,
        id: &str,
        input: &TaskWorkReport,
    ) -> Result<TaskWorkSession, PlatformError> {
        TaskWorkReportStore::report(self, ctx, project, task, id, input)
    }
}
