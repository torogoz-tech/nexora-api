use crate::{PlatformStore, labor_calendar_policy::LaborCalendarPolicy};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn team_members(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        let rows=tx.query("SELECT jsonb_build_object('id',a.id,'name',a.name,'position',COALESCE(p.position,''),'role',r.role,'avatar_revision',COALESCE(v.revision,0)) FROM actors a JOIN users u ON u.actor_id=a.id JOIN actor_roles r ON r.actor_id=a.id LEFT JOIN member_positions p ON p.actor_id=a.id LEFT JOIN user_avatars v ON v.actor_id=a.id WHERE a.kind='human' AND a.active AND u.archived_at IS NULL AND r.role IN ('developer','technical_lead','pmo','manager','super_admin') ORDER BY lower(a.name),a.id",&[]).map_err(Self::database_error)?;
        let members: Vec<Value> = rows.iter().map(|row| row.get(0)).collect();
        tx.commit().map_err(Self::database_error)?;
        Ok(json!(members))
    }
}
