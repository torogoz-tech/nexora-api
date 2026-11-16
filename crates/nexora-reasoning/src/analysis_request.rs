use crate::{Criterion, ReasoningError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisRequest {
    pub purpose: String,
    pub objective: String,
    pub criteria: Vec<Criterion>,
}
impl AnalysisRequest {
    pub fn validate(&self) -> Result<(), ReasoningError> {
        if !matches!(
            self.purpose.as_str(),
            "understanding" | "requirement_connection" | "code_review" | "contribution_review"
        ) || self.objective.trim().is_empty()
            || self.objective.len() > 16000
            || self.criteria.is_empty()
            || self.criteria.len() > 32
        {
            return Err(ReasoningError::InvalidInput);
        }
        let mut ids = BTreeSet::new();
        for c in &self.criteria {
            if c.id.is_empty()
                || c.id.len() > 128
                || !ids.insert(&c.id)
                || c.description.trim().is_empty()
                || c.description.len() > 8000
            {
                return Err(ReasoningError::InvalidInput);
            }
        }
        Ok(())
    }
}
