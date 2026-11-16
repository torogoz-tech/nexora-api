use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PolicyVersion(pub String);
impl PolicyVersion {
    pub fn validate(&self) -> Result<(), crate::GovernanceError> {
        let parts: Vec<_> = self.0.split('.').collect();
        if parts.len() != 3
            || parts.iter().any(|p| {
                p.is_empty()
                    || !p.bytes().all(|b| b.is_ascii_digit())
                    || (p.len() > 1 && p.starts_with('0'))
                    || p.parse::<u32>().is_err()
            })
        {
            return Err(crate::GovernanceError::Configuration(
                "version must be numeric SemVer x.y.z without leading zeros".into(),
            ));
        }
        Ok(())
    }
}
