use crate::{
    PlatformStore, SecretGenerator, document_approval_fingerprint::DocumentApprovalFingerprint,
    document_context_store::DocumentContextStore, document_history_writer::DocumentHistoryWriter,
    document_policy_store::DocumentPolicyStore,
    document_transition_evaluation::DocumentTransitionEvaluation,
    platform_authority::PlatformAuthority,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};
pub(crate) struct DocumentApprovalStore;
impl DocumentApprovalStore {
    #[allow(clippy::too_many_arguments)]
    pub fn grant_in(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        version: &str,
        edge: &str,
        input: &DocumentTransitionRequest,
        reason: &str,
        seconds: u32,
    ) -> Result<DocumentApproval, PlatformError> {
        if ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        if !(1..=86400).contains(&seconds) || input.approval_id.is_some() {
            return Err(PlatformError::Validation(
                "invalid document approval parameters",
            ));
        }
        let reason = DomainText::required(reason, 4000)?;
        PlatformAuthority::require(tx, ctx, Capability::ApprovalsGrant, Some(project))?;
        PlatformAuthority::project(tx, project)?;
        let source = DocumentPolicyStore::version(tx, project, id, version)?;
        DocumentContextStore::lock_version(tx, &source)?;
        let root = DocumentPolicyStore::definition(tx, project, id, true)?;
        DocumentPolicyStore::active(&root, None)?;
        let current = DocumentPolicyStore::version(tx, project, id, version)?;
        if current.state_revision != input.expected_state_revision {
            return Err(PlatformError::Conflict("document state revision changed"));
        }
        let flow = DocumentPolicyStore::flow(
            tx,
            project,
            current
                .workflow_version_id
                .as_deref()
                .ok_or(PlatformError::Validation(
                    "knowledge does not use workflow approvals",
                ))?,
            root.kind,
            false,
        )?;
        let mut evaluation = DocumentTransitionEvaluation::build(&root, &current, input)?;
        evaluation.human_approval_valid = true;
        let transition = flow
            .graph
            .transition(&current.state_id, edge, &evaluation)?;
        if !transition.guards.contains(&WorkflowGuard::HumanApproval) {
            return Err(PlatformError::Validation(
                "document transition does not require human approval",
            ));
        }
        let parameters = DocumentApprovalFingerprint::parameters(input)?;
        let checksum = DocumentApprovalFingerprint::checksum(&root, &current, transition, input)?;
        let approval = SecretGenerator::id("dap")?;
        let seconds = i64::from(seconds);
        let value:Value=tx.query_one("INSERT INTO document_approvals AS a(id,project_id,document_id,version_id,document_revision,state_revision,content_checksum,workflow_version_id,transition_id,from_state,to_state,parameters,parameters_checksum,reason,granted_by,credential_id,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,clock_timestamp()+$17::bigint*interval '1 second') RETURNING to_jsonb(a)",&[&approval,&project,&id,&version,&root.revision,&current.state_revision,&current.checksum,&flow.id,&transition.id,&transition.from,&transition.to,&parameters,&checksum,&reason,&ctx.actor_id(),&ctx.credential_id(),&seconds]).map_err(PlatformStore::database_error)?.get(0);
        DocumentHistoryWriter::append(
            tx,
            ctx,
            project,
            id,
            version,
            "document.approval_granted",
            &reason,
            json!({"approval_id":approval,"transition_id":edge,"parameters_checksum":checksum}),
        )?;
        PlatformStore::decode(value)
    }
    pub fn consume(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        root: &ProjectDocument,
        version: &ProjectDocumentVersion,
        flow: &WorkflowVersion,
        edge: &str,
        input: &DocumentTransitionRequest,
    ) -> Result<bool, PlatformError> {
        let Some(id) = input.approval_id.as_deref() else {
            return Ok(false);
        };
        let transition = flow
            .graph
            .transitions
            .iter()
            .find(|candidate| candidate.id == edge && candidate.from == version.state_id)
            .ok_or(PlatformError::Validation(
                "document transition is not available",
            ))?;
        if !transition.guards.contains(&WorkflowGuard::HumanApproval) {
            return Err(PlatformError::Validation(
                "document transition does not accept an approval",
            ));
        }
        let value:Value=tx.query_opt("SELECT to_jsonb(a) FROM document_approvals a WHERE id=$1 AND project_id=$2 AND document_id=$3 AND version_id=$4 AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at>clock_timestamp() FOR UPDATE",&[&id,&root.project_id,&root.id,&version.id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("document approval is unavailable"))?.get(0);
        let approval: DocumentApproval = PlatformStore::decode(value)?;
        if approval.document_revision != root.revision
            || approval.state_revision != version.state_revision
            || approval.content_checksum != version.checksum
            || approval.workflow_version_id != flow.id
            || approval.transition_id != edge
            || approval.from_state != version.state_id
            || approval.to_state != transition.to
            || approval.parameters_checksum
                != DocumentApprovalFingerprint::checksum(root, version, transition, input)?
        {
            return Err(PlatformError::Conflict(
                "document approval does not match this operation",
            ));
        }
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND active AND kind='human' FOR SHARE",
            &[&approval.granted_by],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::Conflict(
            "document approval grantor is inactive",
        ))?;
        let count=tx.execute("UPDATE document_approvals SET consumed_at=clock_timestamp(),consumed_by=$2,consumed_credential_id=$3 WHERE id=$1 AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at>clock_timestamp()",&[&id,&ctx.actor_id(),&ctx.credential_id()]).map_err(PlatformStore::database_error)?;
        if count != 1 {
            return Err(PlatformError::Conflict("document approval expired"));
        }
        DocumentHistoryWriter::append(
            tx,
            ctx,
            &root.project_id,
            &root.id,
            &version.id,
            "document.approval_consumed",
            &input.reason,
            json!({"approval_id":id,"transition_id":edge,"parameters_checksum":approval.parameters_checksum}),
        )?;
        Ok(true)
    }
}
