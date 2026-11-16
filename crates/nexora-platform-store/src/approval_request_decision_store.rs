use crate::{
    PlatformStore, approval_operation_policy::ApprovalOperationPolicy,
    approval_request_authority::ApprovalRequestAuthority,
    approval_request_history_writer::ApprovalRequestHistoryWriter,
    approval_request_query_store::ApprovalRequestQueryStore,
    document_approval_store::DocumentApprovalStore, receipt_writer::ReceiptWriter,
    task_approval_store::TaskApprovalStore,
};
use nexora_platform::*;
pub(crate) struct ApprovalRequestDecisionStore;
impl ApprovalRequestDecisionStore {
    pub fn decide(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &ApprovalRequestDecide,
    ) -> Result<ApprovalRequest, PlatformError> {
        let reason = DomainText::required(&input.reason, 4000)?;
        if input.expected_revision < 1 || !(1..=86400).contains(&input.approval_expires_in_seconds)
        {
            return Err(PlatformError::Validation(
                "invalid approval decision revision or duration",
            ));
        }
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        ApprovalRequestAuthority::human(&mut tx, ctx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "approval_request.decide",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let selected = ApprovalRequestQueryStore::row(&mut tx, project, id, false)?;
        ApprovalOperationPolicy::lock(&mut tx, project, &selected.target)?;
        let current = ApprovalRequestQueryStore::row(&mut tx, project, id, true)?;
        Self::pending(&current, input.expected_revision)?;
        let remaining:i64=tx.query_one("SELECT floor(extract(epoch FROM expires_at-clock_timestamp()))::bigint FROM approval_requests WHERE id=$1",&[&id]).map_err(PlatformStore::database_error)?.get(0);
        if remaining < 1 {
            return Err(PlatformError::Conflict("approval request expired"));
        }
        let mut task_approval: Option<String> = None;
        let mut document_approval: Option<String> = None;
        let status = match input.decision {
            ApprovalDecision::Rejected => "rejected",
            ApprovalDecision::Approved => {
                let snapshot =
                    ApprovalOperationPolicy::snapshot(&mut tx, project, &current.target)?;
                ApprovalOperationPolicy::matches(&current, &snapshot)?;
                let seconds = i64::from(input.approval_expires_in_seconds).min(remaining) as u32;
                match &current.target {
                    ApprovalRequestTarget::Task {
                        task_id,
                        transition_id,
                        parameters,
                    } => {
                        task_approval = Some(
                            TaskApprovalStore::grant_in(
                                &mut tx,
                                ctx,
                                Some(project),
                                task_id,
                                &TaskApprovalGrant {
                                    transition_id: transition_id.clone(),
                                    parameters: parameters.clone(),
                                    reason: reason.clone(),
                                    expires_in_seconds: seconds,
                                },
                            )?
                            .id,
                        );
                    }
                    ApprovalRequestTarget::DocumentVersion {
                        document_id,
                        version_id,
                        transition_id,
                        parameters,
                    } => {
                        document_approval = Some(
                            DocumentApprovalStore::grant_in(
                                &mut tx,
                                ctx,
                                project,
                                document_id,
                                version_id,
                                transition_id,
                                parameters,
                                &reason,
                                seconds,
                            )?
                            .id,
                        );
                    }
                }
                "approved"
            }
        };
        let count=tx.execute("UPDATE approval_requests SET status=$2,revision=revision+1,task_approval_id=$3,document_approval_id=$4,decided_by=$5,decided_credential_id=$6,decision_reason=$7,decided_at=clock_timestamp() WHERE id=$1 AND status='pending' AND expires_at>clock_timestamp()",&[&id,&status,&task_approval,&document_approval,&ctx.actor_id(),&ctx.credential_id(),&reason]).map_err(PlatformStore::database_error)?;
        if count != 1 {
            return Err(PlatformError::Conflict("approval request expired"));
        }
        let result = ApprovalRequestQueryStore::row(&mut tx, project, id, false)?;
        ApprovalRequestHistoryWriter::append(
            &mut tx,
            ctx,
            &result,
            "approval_request.decided",
            &reason,
        )?;
        ReceiptWriter::save(
            &mut tx,
            receipt.as_ref(),
            &serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?,
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn cancel(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &ApprovalRequestCancel,
    ) -> Result<ApprovalRequest, PlatformError> {
        let reason = DomainText::required(&input.reason, 4000)?;
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let access = ApprovalRequestAuthority::readable(&mut tx, ctx, project)?;
        let selected = ApprovalRequestQueryStore::row(&mut tx, project, id, false)?;
        if (selected.target_kind == "task" && !access.0)
            || (selected.target_kind == "document_version" && !access.1)
        {
            return Err(PlatformError::NotFound);
        }
        if selected.requested_by != ctx.actor_id() {
            ApprovalRequestAuthority::human(&mut tx, ctx, project)?;
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "approval_request.cancel",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        ApprovalOperationPolicy::lock(&mut tx, project, &selected.target)?;
        let current = ApprovalRequestQueryStore::row(&mut tx, project, id, true)?;
        Self::pending(&current, input.expected_revision)?;
        tx.execute("UPDATE approval_requests SET status='cancelled',revision=revision+1,decided_by=$2,decided_credential_id=$3,decision_reason=$4,decided_at=clock_timestamp() WHERE id=$1",&[&id,&ctx.actor_id(),&ctx.credential_id(),&reason]).map_err(PlatformStore::database_error)?;
        let result = ApprovalRequestQueryStore::row(&mut tx, project, id, false)?;
        ApprovalRequestHistoryWriter::append(
            &mut tx,
            ctx,
            &result,
            "approval_request.cancelled",
            &reason,
        )?;
        ReceiptWriter::save(
            &mut tx,
            receipt.as_ref(),
            &serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?,
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    fn pending(request: &ApprovalRequest, revision: i64) -> Result<(), PlatformError> {
        if revision < 1 {
            return Err(PlatformError::Validation(
                "invalid approval request revision",
            ));
        }
        if request.revision != revision || request.status != ApprovalRequestStatus::Pending {
            return Err(PlatformError::Conflict(
                "approval request already decided or revision changed",
            ));
        }
        Ok(())
    }
}
