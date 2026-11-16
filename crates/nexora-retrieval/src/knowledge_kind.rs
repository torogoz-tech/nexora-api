use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    Requirement,
    BusinessRule,
    Decision,
    Document,
    BestPractice,
    TechnicalDebt,
    Feature,
    Task,
    CodeFact,
    Policy,
    AnalysisEvidence,
}

impl KnowledgeKind {
    pub fn requires_embedding(self) -> bool {
        !matches!(self, Self::AnalysisEvidence)
    }
}
