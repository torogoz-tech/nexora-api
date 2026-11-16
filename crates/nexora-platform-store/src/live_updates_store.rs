use crate::{PlatformStore, platform_authority::PlatformAuthority};
use nexora_platform::{Capability, PlatformError};
use std::collections::BTreeMap;
impl PlatformStore {
    /// Re-authenticate on every snapshot, so a running stream cannot outlive a grant/session.
    pub fn live_update_snapshot(
        &mut self,
        token: &str,
        project: Option<&str>,
    ) -> Result<BTreeMap<String, i64>, PlatformError> {
        let ctx = self.authenticate(token)?;
        let mut tx = self
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let mut topics = Vec::new();
        if let Some(project) = project {
            if project.is_empty() || project.len() > 128 {
                return Err(PlatformError::Validation("invalid live update project"));
            }
            for (topic, cap) in [
                ("documents", Capability::DocumentsRead),
                ("knowledge", Capability::KnowledgeRead),
                ("repositories", Capability::RepositoriesRead),
            ] {
                match PlatformAuthority::require(&mut tx, &ctx, cap, Some(project)) {
                    Ok(()) => topics.push(topic.to_string()),
                    Err(PlatformError::Forbidden) => {}
                    Err(e) => return Err(e),
                }
            }
            if topics.is_empty() {
                return Err(PlatformError::NotFound);
            }
            PlatformAuthority::project(&mut tx, project)?;
        } else {
            crate::organization_knowledge_authority::OrganizationKnowledgeAuthority::read(
                &mut tx, &ctx,
            )?;
            topics.push("documents".into());
            if PlatformAuthority::require(&mut tx, &ctx, Capability::InstallationManage, None)
                .is_ok()
            {
                topics.push("task-types".into());
            }
        }
        let mut snapshot = topics
            .iter()
            .map(|t| (t.clone(), 0))
            .collect::<BTreeMap<_, _>>();
        for row in tx.query("SELECT topic,revision FROM live_update_revisions WHERE scope_id=$1 AND topic=ANY($2)",&[&project.unwrap_or("*"),&topics]).map_err(PlatformStore::database_error)? {snapshot.insert(row.get(0),row.get(1));}
        if ctx.method() == nexora_platform::AuthMethod::Session
            && ctx.kind() == nexora_platform::ActorKind::Human
        {
            let revision = tx.query_opt("SELECT revision FROM live_update_revisions WHERE scope_id=$1 AND topic='notifications'", &[&format!("actor:{}", ctx.actor_id())]).map_err(PlatformStore::database_error)?.map_or(0, |row| row.get::<_,i64>(0));
            snapshot.insert("notifications".into(), revision);
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(snapshot)
    }
}
