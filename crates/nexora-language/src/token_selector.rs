use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenSelector {
    pub text: String,
    #[serde(default)]
    pub kind: Option<String>,
}

impl TokenSelector {
    pub(crate) fn overlaps(&self, other: &Self) -> bool {
        self.text == other.text
            && (self.kind.is_none() || other.kind.is_none() || self.kind == other.kind)
    }
}
