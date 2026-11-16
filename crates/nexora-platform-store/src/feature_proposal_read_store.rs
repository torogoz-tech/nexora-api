use crate::{PlatformStore, feature_governance_policy::FeatureGovernancePolicy};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;
impl PlatformStore {
    pub fn feature_proposal(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        FeatureGovernancePolicy::read(&mut tx, ctx, project)?;
        let value = tx
            .query_opt(
                "SELECT to_jsonb(p) FROM feature_proposals p WHERE id=$1 AND project_id=$2",
                &[&id, &project],
            )
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
