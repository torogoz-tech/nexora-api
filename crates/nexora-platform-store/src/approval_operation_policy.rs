use crate::{
    approval_operation_snapshot::ApprovalOperationSnapshot,
    document_approval_fingerprint::DocumentApprovalFingerprint,
    document_context_store::DocumentContextStore, document_policy_store::DocumentPolicyStore,
    document_transition_evaluation::DocumentTransitionEvaluation,
    platform_authority::PlatformAuthority, task_approval_fingerprint::TaskApprovalFingerprint,
    task_policy_store::TaskPolicyStore, task_transition_evaluation::TaskTransitionEvaluation,
};
use nexora_platform::*;
use postgres::Transaction;
pub(crate) struct ApprovalOperationPolicy;
impl ApprovalOperationPolicy {
    pub fn request_authority(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        target: &ApprovalRequestTarget,
    ) -> Result<(), PlatformError> {
        let caps = match target {
            ApprovalRequestTarget::Task { .. } => {
                [Capability::TasksRead, Capability::TasksTransition]
            }
            ApprovalRequestTarget::DocumentVersion { .. } => {
                [Capability::DocumentsRead, Capability::DocumentsWrite]
            }
        };
        for cap in caps {
            PlatformAuthority::require(tx, ctx, cap, Some(project))?;
        }
        PlatformAuthority::project(tx, project)
    }
    pub fn lock(
        tx: &mut Transaction<'_>,
        project: &str,
        target: &ApprovalRequestTarget,
    ) -> Result<(), PlatformError> {
        match target {
            ApprovalRequestTarget::Task { task_id, .. } => {
                TaskPolicyStore::task(tx, project, task_id)?;
            }
            ApprovalRequestTarget::DocumentVersion {
                document_id,
                version_id,
                ..
            } => {
                let version = DocumentPolicyStore::version(tx, project, document_id, version_id)?;
                DocumentContextStore::lock_version(tx, &version)?;
                DocumentPolicyStore::definition(tx, project, document_id, true)?;
            }
        }
        Ok(())
    }
    pub fn snapshot(
        tx: &mut Transaction<'_>,
        project: &str,
        target: &ApprovalRequestTarget,
    ) -> Result<ApprovalOperationSnapshot, PlatformError> {
        match target {
            ApprovalRequestTarget::Task {
                task_id,
                transition_id,
                parameters,
            } => {
                if parameters.approval_id.is_some() {
                    return Err(PlatformError::Validation(
                        "approval request cannot contain a nested approval",
                    ));
                }
                let task = TaskPolicyStore::task(tx, project, task_id)?;
                if task.archived || task.revision != parameters.expected_revision {
                    return Err(PlatformError::Conflict(
                        "requested task revision changed or archived",
                    ));
                }
                let flow = TaskPolicyStore::flow(tx, project, task.execution_version()?, false)?;
                let mut context =
                    TaskTransitionEvaluation::build(tx, &task, parameters, &flow, transition_id)?;
                context.human_approval_valid = true;
                let edge =
                    flow.graph
                        .transition(task.execution_state()?, transition_id, &context)?;
                if !edge.requires_approval() {
                    return Err(PlatformError::Validation(
                        "requested transition does not require human approval",
                    ));
                }
                Ok(ApprovalOperationSnapshot {
                    target_kind: "task",
                    resource_id: task.id.clone(),
                    version_id: None,
                    transition_id: edge.id.clone(),
                    root_revision: task.revision,
                    resource_revision: task.revision,
                    workflow_version_id: flow.id.clone(),
                    from_state: edge.from.clone(),
                    to_state: edge.to.clone(),
                    checksum: TaskApprovalFingerprint::checksum(&task, edge, parameters)?,
                })
            }
            ApprovalRequestTarget::DocumentVersion {
                document_id,
                version_id,
                transition_id,
                parameters,
            } => {
                if parameters.approval_id.is_some() {
                    return Err(PlatformError::Validation(
                        "approval request cannot contain a nested approval",
                    ));
                }
                let before = DocumentPolicyStore::version(tx, project, document_id, version_id)?;
                DocumentContextStore::lock_version(tx, &before)?;
                let root = DocumentPolicyStore::definition(tx, project, document_id, true)?;
                DocumentPolicyStore::active(&root, None)?;
                let version = DocumentPolicyStore::version(tx, project, document_id, version_id)?;
                if version.state_revision != parameters.expected_state_revision {
                    return Err(PlatformError::Conflict(
                        "requested document state revision changed",
                    ));
                }
                let flow = DocumentPolicyStore::flow(
                    tx,
                    project,
                    version
                        .workflow_version_id
                        .as_deref()
                        .ok_or(PlatformError::Validation(
                            "knowledge does not use workflow approvals",
                        ))?,
                    root.kind,
                    false,
                )?;
                let mut context = DocumentTransitionEvaluation::build(&root, &version, parameters)?;
                context.human_approval_valid = true;
                let edge = flow
                    .graph
                    .transition(&version.state_id, transition_id, &context)?;
                if !edge.requires_approval() {
                    return Err(PlatformError::Validation(
                        "requested document transition does not require human approval",
                    ));
                }
                Ok(ApprovalOperationSnapshot {
                    target_kind: "document_version",
                    resource_id: root.id.clone(),
                    version_id: Some(version.id.clone()),
                    transition_id: edge.id.clone(),
                    root_revision: root.revision,
                    resource_revision: version.state_revision,
                    workflow_version_id: flow.id.clone(),
                    from_state: edge.from.clone(),
                    to_state: edge.to.clone(),
                    checksum: DocumentApprovalFingerprint::checksum(
                        &root, &version, edge, parameters,
                    )?,
                })
            }
        }
    }
    pub fn matches(
        request: &ApprovalRequest,
        snapshot: &ApprovalOperationSnapshot,
    ) -> Result<(), PlatformError> {
        if request.target_kind != snapshot.target_kind
            || request.resource_id != snapshot.resource_id
            || request.version_id != snapshot.version_id
            || request.transition_id != snapshot.transition_id
            || request.root_revision != snapshot.root_revision
            || request.resource_revision != snapshot.resource_revision
            || request.workflow_version_id != snapshot.workflow_version_id
            || request.from_state != snapshot.from_state
            || request.to_state != snapshot.to_state
            || request.operation_checksum != snapshot.checksum
        {
            return Err(PlatformError::Conflict(
                "approval request no longer describes the current operation",
            ));
        }
        Ok(())
    }
}
