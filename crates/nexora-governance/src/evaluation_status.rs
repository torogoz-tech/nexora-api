use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EvaluationStatus {
    Pass,
    Fail,
    Unknown,
}
impl EvaluationStatus {
    pub fn all(values: &[Self]) -> Self {
        if values.contains(&Self::Fail) {
            Self::Fail
        } else if values.contains(&Self::Unknown) {
            Self::Unknown
        } else {
            Self::Pass
        }
    }
    pub fn any(values: &[Self]) -> Self {
        if values.contains(&Self::Pass) {
            Self::Pass
        } else if values.contains(&Self::Unknown) {
            Self::Unknown
        } else {
            Self::Fail
        }
    }
    pub fn negate(self) -> Self {
        match self {
            Self::Pass => Self::Fail,
            Self::Fail => Self::Pass,
            Self::Unknown => Self::Unknown,
        }
    }
    pub fn none(values: &[Self]) -> Self {
        Self::any(values).negate()
    }
}
