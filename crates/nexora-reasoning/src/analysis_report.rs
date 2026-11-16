use crate::{AnalysisRequest, CriterionResult, Evidence, ReasoningError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisReport {
    pub summary: String,
    pub criteria: Vec<CriterionResult>,
    pub limitations: Vec<String>,
}
impl AnalysisReport {
    pub fn validate(
        &self,
        request: &AnalysisRequest,
        evidence: &[Evidence],
    ) -> Result<(), ReasoningError> {
        let ids: BTreeSet<_> = request.criteria.iter().map(|c| c.id.as_str()).collect();
        let actual: BTreeSet<_> = self
            .criteria
            .iter()
            .map(|c| c.criterion_id.as_str())
            .collect();
        if ids != actual || self.criteria.len() != ids.len() || self.summary.trim().is_empty() {
            return Err(ReasoningError::InvalidResponse);
        }
        for c in &self.criteria {
            if request.purpose == "contribution_review"
                && c.outcome != "insufficient_evidence"
                && !c.evidence_ids.iter().any(|id| {
                    evidence
                        .iter()
                        .any(|e| &e.id == id && e.tool == crate::ComparisonTool::NAME)
                })
            {
                return Err(ReasoningError::Evidence);
            }

            if !matches!(
                c.outcome.as_str(),
                "supported" | "contradicted" | "insufficient_evidence"
            ) || c.explanation.trim().is_empty()
            {
                return Err(ReasoningError::InvalidResponse);
            }
            if c.evidence_ids
                .iter()
                .any(|id| !evidence.iter().any(|e| &e.id == id))
                || (c.outcome != "insufficient_evidence" && c.evidence_ids.is_empty())
                || (c.outcome == "insufficient_evidence" && c.missing_evidence.is_empty())
            {
                return Err(ReasoningError::Evidence);
            }
        }
        Ok(())
    }
}
