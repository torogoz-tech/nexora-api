use nexora_platform::*;
use serde_json::json;
pub(crate) struct DocumentTransitionEvaluation;
impl DocumentTransitionEvaluation {
    pub fn build(
        root: &ProjectDocument,
        version: &ProjectDocumentVersion,
        input: &DocumentTransitionRequest,
    ) -> Result<WorkflowEvaluationContext, PlatformError> {
        if input.expected_state_revision < 1 {
            return Err(PlatformError::Validation(
                "expected document state revision must be positive",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let mut fields = version.content["body"]
            .as_object()
            .cloned()
            .ok_or(PlatformError::Storage)?;
        fields.insert("title".into(), json!(version.title));
        fields.insert("code".into(), json!(root.code));
        Ok(WorkflowEvaluationContext {
            fields,
            reason: Some(reason),
            evidence_valid: !version.evidence.is_empty() && !version.needs_review,
            ..Default::default()
        })
    }
}
