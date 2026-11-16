use crate::PlatformStore;
use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_platform::PlatformError;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn claim_knowledge_synthesis(&mut self) -> Result<Option<Value>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        // Uncertain outcomes are never resent automatically: prevent surprise token consumption.
        tx.execute("UPDATE knowledge_contributions SET status='failed',updated_at=now() WHERE status='processing' AND claimed_at<now()-interval '3 minutes'",&[]).map_err(Self::database_error)?;
        let row=tx.query_opt("SELECT c.id,c.filename,c.mime_type,c.payload,c.sha256,c.source_revision,(SELECT language FROM installation_preferences LIMIT 1) FROM knowledge_contributions c JOIN knowledge_contribution_sources src ON src.contribution_id=c.id AND src.source_revision=c.source_revision JOIN actors a ON a.id=src.created_by WHERE c.status='queued' AND a.active AND EXISTS(SELECT 1 FROM actor_roles r WHERE r.actor_id=a.id) AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=a.id AND g.capability='knowledge:contribute' AND (g.project_id IS NULL OR g.project_id=c.project_id)) AND ((c.project_id IS NULL AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=a.id AND g.capability='organization-knowledge:read' AND g.project_id IS NULL)) OR (c.project_id IS NOT NULL AND EXISTS(SELECT 1 FROM projects p WHERE p.id=c.project_id AND NOT p.archived) AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=a.id AND g.capability='documents:read' AND (g.project_id IS NULL OR g.project_id=c.project_id)))) ORDER BY c.created_at,c.id FOR UPDATE OF c SKIP LOCKED LIMIT 1",&[]).map_err(Self::database_error)?;
        let value = if let Some(row) = row {
            let id: String = row.get(0);
            let bytes: Vec<u8> = row.get(3);
            let checksum: String = row.get(4);
            if crate::SecretGenerator::checksum(&bytes) != checksum {
                return Err(PlatformError::Storage);
            }
            tx.execute("UPDATE knowledge_contributions SET status='processing',attempts=attempts+1,claimed_at=now(),updated_at=now() WHERE id=$1",&[&id]).map_err(Self::database_error)?;
            Some(
                json!({"id":id,"filename":row.get::<_,String>(1),"mime_type":row.get::<_,String>(2),"data_base64":STANDARD.encode(bytes),"source_revision":row.get::<_,i64>(5),"language":row.get::<_,Option<String>>(6).unwrap_or("es".into())}),
            )
        } else {
            None
        };
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn finish_knowledge_synthesis(
        &mut self,
        id: &str,
        source_revision: i64,
        body: Option<&str>,
    ) -> Result<(), PlatformError> {
        let body = body.filter(|s| !s.trim().is_empty() && s.len() <= 16000);
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        if let Some(body) = body {
            let changed=tx.execute("UPDATE knowledge_contributions SET body=$2,status='ready',revision=revision+1,updated_at=now() WHERE id=$1 AND status='processing' AND source_revision=$3",&[&id,&body,&source_revision]).map_err(Self::database_error)?;
            if changed == 1 {
                tx.execute("INSERT INTO knowledge_contribution_versions(contribution_id,revision,source_revision,title,body) SELECT id,revision,source_revision,title,body FROM knowledge_contributions WHERE id=$1",&[&id]).map_err(Self::database_error)?;
                tx.execute("INSERT INTO knowledge_processing_jobs(id,project_id,source_kind,resource_id,source_revision,content_hash,status) SELECT 'krag-'||id||'-'||revision,project_id,kind,id,revision,COALESCE(sha256,md5(title||body)),'queued' FROM knowledge_contributions WHERE id=$1 ON CONFLICT(source_kind,resource_id,source_revision,content_hash) DO UPDATE SET status=CASE WHEN knowledge_processing_jobs.status IN ('failed','cancelled') THEN 'queued' ELSE knowledge_processing_jobs.status END,updated_at=clock_timestamp()", &[&id]).map_err(Self::database_error)?;
            }
        } else {
            tx.execute("UPDATE knowledge_contributions SET status='failed',updated_at=now() WHERE id=$1 AND status='processing' AND source_revision=$2",&[&id,&source_revision]).map_err(Self::database_error)?;
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(())
    }
}
